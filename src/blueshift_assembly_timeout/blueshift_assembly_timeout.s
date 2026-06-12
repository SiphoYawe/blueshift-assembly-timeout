// ---------------------------------------------------------------------------
//  Assembly Timeout — slot-height deadline guard
//  Blueshift challenge: https://learn.blueshift.gg/en/challenges/assembly-timeout
//
//  Verifier contract (differs from the challenge page, which still teaches a
//  sol_get_clock_sysvar solution — 140 CUs for the syscall alone, while the
//  live verifier enforces "max 4" compute units on the success path):
//    - Account #1 is the Clock sysvar; its 40-byte data starts at input
//      offset 0x0060, so the current slot (first u64 of Clock) is at 0x0060.
//    - Instruction data follows the serialized account: 8-byte u64
//      max_slot_height at offset 0x2898
//      (= 8 count + 8 acct header + 32 key + 32 owner + 8 lamports
//         + 8 data_len + 40 data + 10240 realloc pad + 8 rent_epoch + 8 len).
//    - Return 0 if current_slot <= max_slot_height, else 1.
//
//  Success path is exactly 4 CUs (ldxdw, ldxdw, jle, exit) — r0 is
//  zero-initialized by the VM, so the happy path never has to touch it.
//  Failure path is 5 CUs.
// ---------------------------------------------------------------------------

.equ CLOCK_SLOT, 0x0060       // r1 + 0x60  -> u64 current slot (Clock data)
.equ MAX_SLOT_HEIGHT, 0x2898  // r1 + 0x2898 -> u64 caller-supplied deadline

.globl entrypoint
entrypoint:
    // Caller-supplied deadline from instruction data.
    ldxdw r2, [r1+MAX_SLOT_HEIGHT]

    // Current slot, read directly from the Clock sysvar account's data.
    ldxdw r1, [r1+CLOCK_SLOT]

    // Inside the deadline window (current <= max): exit with r0 = 0,
    // untouched since VM entry.
    jle r1, r2, end

    // Deadline missed: error code 1.
    lddw r0, 1

end:
    exit
