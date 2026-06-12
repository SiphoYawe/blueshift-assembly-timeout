// ---------------------------------------------------------------------------
//  Assembly Timeout — slot-height deadline guard
//  Blueshift challenge: https://learn.blueshift.gg/en/challenges/assembly-timeout
//
//  Contract:
//    - Accepts zero accounts.
//    - Reads an 8-byte u64 max_slot_height from instruction data.
//    - Calls sol_get_clock_sysvar to read the current slot.
//    - Returns 0 if current_slot <= max_slot_height, else 1.
// ---------------------------------------------------------------------------

.equ NUM_ACCOUNTS, 0x0000        // r1 + 0x00 -> u64 account count
.equ MAX_SLOT_HEIGHT, 0x0010     // r1 + 0x10 -> u64 caller-supplied deadline
.equ CURRENT_SLOT_HEIGHT, -0x0028 // r10 - 40 -> base of 40-byte Clock buffer

.globl entrypoint
entrypoint:
    // Load num_accounts into r0 and veto immediately if non-zero, using the
    // count itself as the error code. The branch must happen BEFORE the
    // sysvar syscall: syscalls return their status in r0, so the canonical
    // "let the count ride in r0" trick is silently erased by the call.
    ldxdw r0, [r1+NUM_ACCOUNTS]
    jne r0, 0, end

    // Pull the caller-supplied deadline into r2.
    ldxdw r2, [r1+MAX_SLOT_HEIGHT]

    // Carve 40 stack bytes for the Clock sysvar. r10 is read-only, so copy
    // it into r1 and add the negative offset (r1 = r10 - 40).
    mov64 r1, r10
    add64 r1, CURRENT_SLOT_HEIGHT

    // Syscall writes the 40-byte Clock struct at [r1].
    call sol_get_clock_sysvar

    // slot is the first u64 field of Clock -> offset 0x00.
    ldxdw r1, [r1+0x0000]

    // Inside the deadline window (current <= max): exit with r0 = 0,
    // which is the syscall's own success return value.
    jle r1, r2, end

    // Deadline missed: error code 1. mov64 imm encodes in one slot vs two
    // for lddw — one CU cheaper on the failure path.
    mov64 r0, 1

end:
    exit
