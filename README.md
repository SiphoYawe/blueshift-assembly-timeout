# blueshift-assembly-timeout

A Solana slot-height deadline guard in **10 sBPF assembly instructions**, built for the
[Blueshift Assembly Timeout challenge](https://learn.blueshift.gg/en/challenges/assembly-timeout).

Append this instruction to any transaction and the whole transaction fails if it lands
after `max_slot_height` — a fail-safe against stale arbitrage, delayed execution, and
instruction replay.

```
success path   149 CUs   (140 of which is the sol_get_clock_sysvar syscall)
failure path   150 CUs
account veto     3 CUs   (branches before the syscall — never pays for the clock)
binary size    1096 bytes
```

## The interesting part: the canonical version has a broken veto

The reference implementation on the challenge page loads the account count into `r0`
and relies on it doubling as the exit code — "any non-zero value automatically fails
the program." Then it calls `sol_get_clock_sysvar`.

**Syscalls return their status in `r0`.** The clock syscall returns `0` on success and
silently erases the account-count veto. Pass one account and the canonical program
reads the first 8 bytes of that account's pubkey as the "deadline" (offset `0x10` no
longer points at instruction data once accounts are serialized in front of it) — and
happily exits `0`.

My Mollusk test suite caught this red-handed: `fails_when_one_account_passed` and
`fails_when_two_accounts_passed` both observed `Success` against the canonical
ordering. The fix is one instruction — branch **before** the syscall:

```asm
ldxdw r0, [r1+NUM_ACCOUNTS]
jne r0, 0, end              // fail-closed, count is the error code
```

Bonus: the veto path now exits in 3 CUs instead of paying the 140-CU syscall first.

## The program

```asm
.equ NUM_ACCOUNTS, 0x0000         // r1 + 0x00 -> u64 account count
.equ MAX_SLOT_HEIGHT, 0x0010      // r1 + 0x10 -> u64 caller-supplied deadline
.equ CURRENT_SLOT_HEIGHT, -0x0028 // r10 - 40  -> base of 40-byte Clock buffer

.globl entrypoint
entrypoint:
    ldxdw r0, [r1+NUM_ACCOUNTS]   // veto any call that passes accounts...
    jne r0, 0, end                // ...BEFORE the syscall can clobber r0

    ldxdw r2, [r1+MAX_SLOT_HEIGHT]

    mov64 r1, r10                 // r10 is read-only: copy, then offset
    add64 r1, CURRENT_SLOT_HEIGHT // r1 = r10 - 40 (stack Clock buffer)
    call sol_get_clock_sysvar     // writes 40-byte Clock struct at [r1]
    ldxdw r1, [r1+0x0000]         // slot = first u64 of Clock

    jle r1, r2, end               // current <= deadline: exit 0 (syscall's return)
    mov64 r0, 1                   // deadline missed (mov64: 1 slot, lddw costs 2)

end:
    exit
```

Disassembly of the shipped `.so` round-trips to exactly these 10 instructions —
no linker artifacts (`sbpf disassemble deploy/blueshift_assembly_timeout.so`).

## Tests

Written test-first against the scaffold's noop (watched 6 of them fail for the right
reasons before writing a line of assembly):

| Test | What it pins down |
|------|-------------------|
| `passes_when_current_slot_below_deadline` | happy path |
| `passes_when_current_slot_equals_deadline` | boundary — `jle` is inclusive |
| `fails_when_current_slot_exceeds_deadline` | exact error: `ProgramError::Custom(1)` |
| `fails_when_one_account_passed` | the veto (catches the canonical r0-clobber bug) |
| `fails_when_two_accounts_passed` | veto with count > 1 |
| `passes_with_max_u64_deadline` | no overflow at `u64::MAX` |
| `fails_when_max_slot_is_zero_and_current_nonzero` | degenerate deadline |
| `cu_budget_in_success_path` | regression guard: ≤ 149 CUs |
| `cu_budget_in_failure_path` | regression guard: ≤ 150 CUs |
| `cu_budget_in_account_veto_path` | regression guard: ≤ 3 CUs |

## Reproduce

```sh
cargo install --git https://github.com/blueshift-gg/sbpf.git
sbpf build        # ~1.5 ms
cargo test        # 10/10 via Mollusk (Agave compute model)
```

Interactive single-stepping with the bundled fixture:

```sh
sbpf debug --elf deploy/blueshift_assembly_timeout.so --input fixtures/pass.json
```

## CU economics, honestly

The syscall sets a hard floor: `sol_get_clock_sysvar` costs 140 CUs no matter what
language you write in. Assembly gets the rest of the program down to 9 instructions —
within 9 CUs of the floor. The win over frameworks isn't a magic 50× number; it's
that nothing else (entrypoint deserialization, account validation machinery,
dispatch) is left to pay for.

## Stack

- [sbpf](https://github.com/blueshift-gg/sbpf) — assembler, scaffold, debugger by [@deanmlittle](https://github.com/deanmlittle) & Blueshift
- [Mollusk](https://github.com/buffalojoec/mollusk) — SVM test harness
- [Blueshift](https://learn.blueshift.gg) — the challenge and the assembly course

## License

MIT
