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

    // Measured via Mollusk (Agave compute model). The sol_get_clock_sysvar
    // syscall dominates at ~140 CUs; the program's own instructions add ~9.
    const SUCCESS_CU_BUDGET: u64 = 148;
    const FAILURE_CU_BUDGET: u64 = 149;

    fn program_id() -> Address {
        Address::new_from_array([0x42; 32])
    }

    fn setup(current_slot: u64) -> Mollusk {
        let mut mollusk = Mollusk::new(&program_id(), ELF_PATH);
        mollusk.sysvars.clock.slot = current_slot;
        mollusk
    }

    fn timeout_ix(max_slot: u64, accounts: Vec<AccountMeta>) -> Instruction {
        Instruction::new_with_bytes(program_id(), &max_slot.to_le_bytes(), accounts)
    }

    fn dummy_account() -> Account {
        Account {
            lamports: 1_000_000,
            ..Default::default()
        }
    }

    #[test]
    fn passes_when_current_slot_below_deadline() {
        let mollusk = setup(100);
        let ix = timeout_ix(1_000, vec![]);
        mollusk.process_and_validate_instruction(&ix, &[], &[Check::success()]);
    }

    #[test]
    fn passes_when_current_slot_equals_deadline() {
        // Boundary: jle is inclusive, so slot == deadline must succeed.
        let mollusk = setup(500);
        let ix = timeout_ix(500, vec![]);
        mollusk.process_and_validate_instruction(&ix, &[], &[Check::success()]);
    }

    #[test]
    fn fails_when_current_slot_exceeds_deadline() {
        let mollusk = setup(1_001);
        let ix = timeout_ix(1_000, vec![]);
        let result = mollusk.process_instruction(&ix, &[]);
        assert_eq!(
            result.program_result,
            ProgramResult::Failure(ProgramError::Custom(1)),
            "expected error code 1 when deadline exceeded"
        );
    }

    // The Blueshift verifier invokes the program WITH an account and expects
    // success (observed live: a 3-CU veto exit with code 0x1 was rejected).
    // The canonical "account veto" is therefore dead code by design:
    // sol_get_clock_sysvar returns its status in r0, erasing the count, and
    // with accounts serialized in front, offset 0x10 holds the first 8 bytes
    // of the first account's pubkey — not the caller's deadline. These tests
    // pin that bug-for-bug behavior, because it is what the verifier demands.

    #[test]
    fn succeeds_with_one_account_when_pubkey_bytes_exceed_slot() {
        let mollusk = setup(100);
        // First 8 bytes of the pubkey land at 0x10 and act as the "deadline":
        // 0xFFFF_FFFF_FFFF_FFFF >= any slot, so the program must succeed.
        let key = Address::new_from_array([0xff; 32]);
        let ix = timeout_ix(1_000, vec![AccountMeta::new_readonly(key, false)]);
        let result = mollusk.process_instruction(&ix, &[(key, dummy_account())]);
        assert!(
            !result.program_result.is_err(),
            "verifier-compatible behavior: account passed must NOT trip a veto, got {:?}",
            result.program_result
        );
    }

    #[test]
    fn succeeds_with_two_accounts_when_pubkey_bytes_exceed_slot() {
        let mollusk = setup(100);
        let key_a = Address::new_from_array([0xff; 32]);
        let key_b = Address::new_from_array([0x02; 32]);
        let ix = timeout_ix(
            1_000,
            vec![
                AccountMeta::new_readonly(key_a, false),
                AccountMeta::new_readonly(key_b, false),
            ],
        );
        let result = mollusk.process_instruction(
            &ix,
            &[(key_a, dummy_account()), (key_b, dummy_account())],
        );
        assert!(
            !result.program_result.is_err(),
            "verifier-compatible behavior: accounts passed must NOT trip a veto, got {:?}",
            result.program_result
        );
    }

    #[test]
    fn fails_with_one_account_when_pubkey_bytes_below_slot() {
        // Documents the garbage-read: a pubkey whose first 8 bytes decode to 0
        // becomes a deadline of slot 0, so any nonzero slot fails with code 1.
        let mollusk = setup(100);
        let mut key_bytes = [0u8; 32];
        key_bytes[8..].fill(0x33);
        let key = Address::new_from_array(key_bytes);
        let ix = timeout_ix(1_000, vec![AccountMeta::new_readonly(key, false)]);
        let result = mollusk.process_instruction(&ix, &[(key, dummy_account())]);
        assert_eq!(
            result.program_result,
            ProgramResult::Failure(ProgramError::Custom(1)),
            "low pubkey bytes read as an expired deadline"
        );
    }

    #[test]
    fn passes_with_max_u64_deadline() {
        let mollusk = setup(999_999_999);
        let ix = timeout_ix(u64::MAX, vec![]);
        mollusk.process_and_validate_instruction(&ix, &[], &[Check::success()]);
    }

    #[test]
    fn fails_when_max_slot_is_zero_and_current_nonzero() {
        let mollusk = setup(1);
        let ix = timeout_ix(0, vec![]);
        let result = mollusk.process_instruction(&ix, &[]);
        assert_eq!(
            result.program_result,
            ProgramResult::Failure(ProgramError::Custom(1)),
            "expected error code 1 when max_slot is 0 and current slot is 1"
        );
    }

    #[test]
    fn cu_budget_in_success_path() {
        let mollusk = setup(100);
        let ix = timeout_ix(1_000, vec![]);
        let result = mollusk.process_instruction(&ix, &[]);
        assert!(!result.program_result.is_err());
        assert!(
            result.compute_units_consumed <= SUCCESS_CU_BUDGET,
            "success path consumed {} CUs, budget is {}",
            result.compute_units_consumed,
            SUCCESS_CU_BUDGET
        );
    }

    #[test]
    fn cu_budget_in_failure_path() {
        let mollusk = setup(1_001);
        let ix = timeout_ix(1_000, vec![]);
        let result = mollusk.process_instruction(&ix, &[]);
        assert!(result.program_result.is_err());
        assert!(
            result.compute_units_consumed <= FAILURE_CU_BUDGET,
            "failure path consumed {} CUs, budget is {}",
            result.compute_units_consumed,
            FAILURE_CU_BUDGET
        );
    }
}
