#[cfg(test)]
mod tests {
    use mollusk_svm::{
        result::{Check, ProgramResult},
        Mollusk,
    };
    use solana_account::Account;
    use solana_address::Address;
    use solana_instruction::{AccountMeta, Instruction};
    use solana_program_error::ProgramError;

    const ELF_PATH: &str = "deploy/blueshift_assembly_timeout";

    // The live Blueshift verifier enforces these (observed: "Exceeded compute
    // units: used 148, max 4"). The syscall solution from the challenge page
    // (140 CUs for sol_get_clock_sysvar alone) cannot pass; the verifier
    // passes the Clock sysvar as account #1 and expects a direct read of its
    // data: slot at input offset 0x0060, instruction data at 0x2898.
    const SUCCESS_CU_BUDGET: u64 = 4;
    const FAILURE_CU_BUDGET: u64 = 5;

    const CLOCK_SYSVAR_ID: &str = "SysvarC1ock11111111111111111111111111111111";
    const SYSVAR_OWNER_ID: &str = "Sysvar1111111111111111111111111111111111111";

    fn program_id() -> Address {
        Address::new_from_array([0x42; 32])
    }

    fn setup(current_slot: u64) -> Mollusk {
        let mut mollusk = Mollusk::new(&program_id(), ELF_PATH);
        mollusk.sysvars.clock.slot = current_slot;
        mollusk
    }

    fn clock_id() -> Address {
        CLOCK_SYSVAR_ID.parse().unwrap()
    }

    // 40-byte bincode layout: slot, epoch_start_timestamp, epoch,
    // leader_schedule_epoch, unix_timestamp — all 8-byte LE.
    fn clock_account(mollusk: &Mollusk) -> (Address, Account) {
        let clock = &mollusk.sysvars.clock;
        let mut data = Vec::with_capacity(40);
        data.extend_from_slice(&clock.slot.to_le_bytes());
        data.extend_from_slice(&clock.epoch_start_timestamp.to_le_bytes());
        data.extend_from_slice(&clock.epoch.to_le_bytes());
        data.extend_from_slice(&clock.leader_schedule_epoch.to_le_bytes());
        data.extend_from_slice(&clock.unix_timestamp.to_le_bytes());
        (
            clock_id(),
            Account {
                lamports: 1_000_000,
                data,
                owner: SYSVAR_OWNER_ID.parse().unwrap(),
                executable: false,
                rent_epoch: 0,
            },
        )
    }

    fn timeout_ix(max_slot: u64) -> Instruction {
        Instruction::new_with_bytes(
            program_id(),
            &max_slot.to_le_bytes(),
            vec![AccountMeta::new_readonly(clock_id(), false)],
        )
    }

    fn run(current_slot: u64, max_slot: u64) -> mollusk_svm::result::InstructionResult {
        let mollusk = setup(current_slot);
        let accounts = [clock_account(&mollusk)];
        mollusk.process_instruction(&timeout_ix(max_slot), &accounts)
    }

    #[test]
    fn passes_when_current_slot_below_deadline() {
        let mollusk = setup(100);
        let accounts = [clock_account(&mollusk)];
        mollusk.process_and_validate_instruction(
            &timeout_ix(1_000),
            &accounts,
            &[Check::success()],
        );
    }

    #[test]
    fn passes_when_current_slot_equals_deadline() {
        // Boundary: jle is inclusive, so slot == deadline must succeed.
        let result = run(500, 500);
        assert!(
            !result.program_result.is_err(),
            "slot == deadline must succeed, got {:?}",
            result.program_result
        );
    }

    #[test]
    fn fails_when_current_slot_exceeds_deadline() {
        let result = run(1_001, 1_000);
        assert_eq!(
            result.program_result,
            ProgramResult::Failure(ProgramError::Custom(1)),
            "expected error code 1 when deadline exceeded"
        );
    }

    #[test]
    fn passes_with_max_u64_deadline() {
        let result = run(999_999_999, u64::MAX);
        assert!(
            !result.program_result.is_err(),
            "u64::MAX deadline must always succeed, got {:?}",
            result.program_result
        );
    }

    #[test]
    fn fails_when_max_slot_is_zero_and_current_nonzero() {
        let result = run(1, 0);
        assert_eq!(
            result.program_result,
            ProgramResult::Failure(ProgramError::Custom(1)),
            "expected error code 1 when max_slot is 0 and current slot is 1"
        );
    }

    #[test]
    fn cu_budget_in_success_path() {
        let result = run(100, 1_000);
        assert!(!result.program_result.is_err());
        assert!(
            result.compute_units_consumed <= SUCCESS_CU_BUDGET,
            "success path consumed {} CUs, verifier max is {}",
            result.compute_units_consumed,
            SUCCESS_CU_BUDGET
        );
    }

    #[test]
    fn cu_budget_in_failure_path() {
        let result = run(1_001, 1_000);
        assert!(result.program_result.is_err());
        assert!(
            result.compute_units_consumed <= FAILURE_CU_BUDGET,
            "failure path consumed {} CUs, budget is {}",
            result.compute_units_consumed,
            FAILURE_CU_BUDGET
        );
    }
}
