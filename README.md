# blueshift-assembly-timeout

A Solana slot-height deadline guard in **9 sBPF assembly instructions**, built for the
[Blueshift Assembly Timeout challenge](https://learn.blueshift.gg/en/challenges/assembly-timeout).

Append this instruction to any transaction and the whole transaction fails if it lands
after `max_slot_height` — a fail-safe against stale arbitrage, delayed execution, and
instruction replay.

```
success path   148 CUs   (140 of which is the sol_get_clock_sysvar syscall)
failure path   149 CUs
binary size    1096 bytes
```

## The interesting part: the "broken" veto is load-bearing

The implementation loads the account count into `r0` and relies on it doubling as the
exit code — "any non-zero value automatically fails the program." Then it calls
`sol_get_clock_sysvar`.

**Syscalls return their status in `r0`.** The clock syscall returns `0` on success and
silently erases the account-count veto. Worse, with accounts serialized in front of the
instruction data, offset `0x10` no longer holds the caller's deadline — it holds the
first 8 bytes of the first account's pubkey, which the program duly compares against
the current slot.

My Mollusk TDD suite caught this immediately: tests that passed 1–2 accounts observed
`Success` where the documented veto promised failure. So I "fixed" it — branch before
the syscall, `jne r0, 0, end`, fail-closed in 3 CUs.

**The Blueshift verifier rejected the fix.** Live log, 3 compute units, error `0x1`:
its success vector invokes the program *with* an account and expects it to pass —
behavior that only works because the syscall clobbers `r0` and the pubkey bytes at
`0x10` happen to exceed the current slot. The bug is not a bug to the verifier; it is
the spec. I reverted to byte-for-byte canonical and pinned the actual behavior in
tests instead, including the garbage-deadline read.

Two lessons I'm keeping:

1. **A test harness tells you what code does; only the integration target tells you
   what it must do.** Mollusk found a real semantic landmine — and the verifier proved
   the landmine was contractual.
2. If you use this pattern outside a challenge, put the veto before the syscall.
   On-chain, fail-closed beats fail-open every time someone passes an account you
   didn't expect.

## The program

```asm
.equ NUM_ACCOUNTS, 0x0000         // r1 + 0x00 -> u64 account count
.equ MAX_SLOT_HEIGHT, 0x0010      // r1 + 0x10 -> u64 caller-supplied deadline
.equ CURRENT_SLOT_HEIGHT, -0x0028 // r10 - 40  -> base of 40-byte Clock buffer

.globl entrypoint
entrypoint:
    ldxdw r0, [r1+NUM_ACCOUNTS]   // would-be veto (see above: clobbered later)
    ldxdw r2, [r1+MAX_SLOT_HEIGHT]

    mov64 r1, r10                 // r10 is read-only: copy, then offset
    add64 r1, CURRENT_SLOT_HEIGHT // r1 = r10 - 40 (stack Clock buffer)
    call sol_get_clock_sysvar     // writes Clock at [r1], returns 0 in r0
    ldxdw r1, [r1+0x0000]         // slot = first u64 of Clock

    jle r1, r2, end               // current <= deadline: exit 0
    lddw r0, 1                    // deadline missed

end:
    exit
```

Disassembly of the shipped `.so` round-trips to exactly these 9 instructions —
no linker artifacts (`sbpf disassemble deploy/blueshift_assembly_timeout.so`).

## Tests

Written test-first against the scaffold's noop (watched them fail for the right
reasons before writing a line of assembly), then rewritten once against live
verifier evidence:

| Test | What it pins down |
|------|-------------------|
| `passes_when_current_slot_below_deadline` | happy path |
| `passes_when_current_slot_equals_deadline` | boundary — `jle` is inclusive |
| `fails_when_current_slot_exceeds_deadline` | exact error: `ProgramError::Custom(1)` |
| `succeeds_with_one_account_when_pubkey_bytes_exceed_slot` | verifier-required r0-clobber behavior |
| `succeeds_with_two_accounts_when_pubkey_bytes_exceed_slot` | same, count > 1 |
| `fails_with_one_account_when_pubkey_bytes_below_slot` | the garbage-deadline read, made visible |
| `passes_with_max_u64_deadline` | no overflow at `u64::MAX` |
| `fails_when_max_slot_is_zero_and_current_nonzero` | degenerate deadline |
| `cu_budget_in_success_path` | regression guard: ≤ 148 CUs |
| `cu_budget_in_failure_path` | regression guard: ≤ 149 CUs |

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
language you write in. Assembly gets the rest of the program down to 8–9 executed
instructions — within 9 CUs of the floor. The win over frameworks isn't a magic 50×
number; it's that nothing else (entrypoint deserialization, account validation
machinery, dispatch) is left to pay for.

## Stack

- [sbpf](https://github.com/blueshift-gg/sbpf) — assembler, scaffold, debugger by [@deanmlittle](https://github.com/deanmlittle) & Blueshift
- [Mollusk](https://github.com/buffalojoec/mollusk) — SVM test harness
- [Blueshift](https://learn.blueshift.gg) — the challenge and the assembly course

## License

MIT
