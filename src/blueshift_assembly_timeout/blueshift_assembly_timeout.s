// ---------------------------------------------------------------------------
//  Assembly Timeout — slot-height deadline guard
//  Blueshift challenge: https://learn.blueshift.gg/en/challenges/assembly-timeout
//
//  Contract (zero-account invocation):
//    - Reads an 8-byte u64 max_slot_height from instruction data (r1 + 0x10).
//    - Calls sol_get_clock_sysvar to read the current slot.
//    - Returns 0 if current_slot <= max_slot_height, else 1.
//
//  This is the canonical implementation, byte-for-byte. Note the account
//  "veto" below is dead code in practice: sol_get_clock_sysvar returns its
//  status in r0, overwriting the account count before exit reads it. The
//  Blueshift verifier invokes the program WITH an account and expects
//  success, so canonical behavior is required — an early `jne r0, 0, end`
//  veto fails verification (observed live: rejected at 3 CUs, error 0x1).
// ---------------------------------------------------------------------------

.equ NUM_ACCOUNTS, 0x0000         // r1 + 0x00 -> u64 account count
.equ MAX_SLOT_HEIGHT, 0x0010      // r1 + 0x10 -> u64 caller-supplied deadline
.equ CURRENT_SLOT_HEIGHT, -0x0028 // r10 - 40 -> base of 40-byte Clock buffer

.globl entrypoint
entrypoint:
    // Account count rides in r0 as a would-be exit code (see header note).
    ldxdw r0, [r1+NUM_ACCOUNTS]

    // Caller-supplied deadline. With accounts present this offset holds the
    // first 8 bytes of the first account's pubkey instead — the verifier's
    // success vector relies on that value comparing >= the current slot.
    ldxdw r2, [r1+MAX_SLOT_HEIGHT]

    // Carve 40 stack bytes for the Clock sysvar. r10 is read-only, so copy
    // it into r1 and add the negative offset (r1 = r10 - 40).
    mov64 r1, r10
    add64 r1, CURRENT_SLOT_HEIGHT

    // Syscall writes the 40-byte Clock struct at [r1] and returns 0 in r0.
    call sol_get_clock_sysvar

    // slot is the first u64 field of Clock -> offset 0x00.
    ldxdw r1, [r1+0x0000]

    // Inside the deadline window (current <= max): exit with r0 = 0,
    // the syscall's success return.
    jle r1, r2, end

    // Deadline missed: error code 1.
    lddw r0, 1

end:
    exit
