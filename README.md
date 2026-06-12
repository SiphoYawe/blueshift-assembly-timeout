# blueshift-assembly-timeout

A Solana slot-height deadline guard in **5 sBPF assembly instructions**, built for the
[Blueshift Assembly Timeout challenge](https://learn.blueshift.gg/en/challenges/assembly-timeout).

Append this instruction to any transaction and the whole transaction fails if it lands
after `max_slot_height` — a fail-safe against stale arbitrage, delayed execution, and
instruction replay.

```
success path     4 CUs   (the verifier's exact cap)
failure path     5 CUs
binary size    320 bytes
```

## The story: two live rejections, three programs

**Program 1 — the documented contract, plus a "fix".** The challenge page teaches a
`sol_get_clock_sysvar` solution whose account-count veto parks the count in `r0`.
My Mollusk TDD suite caught that the veto is dead code: syscalls return their status
in `r0`, so the clock syscall erases the count before `exit` reads it. I reordered the
veto before the syscall — fail-closed in 3 CUs. The verifier rejected it: error `0x1`,
3 CUs consumed. Three compute units is a fingerprint — my veto fired on the verifier's
*success* vector, which invokes the program **with an account attached**.

**Program 2 — byte-for-byte canonical.** Re-uploaded the challenge page's exact
program. Rejected again: `Exceeded compute units: used 148, max 4`. The program
*succeeded* — but the verifier caps the success path at **4 CUs**, and
`sol_get_clock_sysvar` alone costs 140. The verifier does not accept the solution its
own course page teaches.

**Program 3 — the real contract.** A 4-CU budget with an account attached can mean
only one thing: the account *is* the Clock sysvar, and you read the slot straight out
of its data. With one 40-byte-data account, the input region lays out as: account
data at `0x0060` (slot = first u64 of Clock), instruction data at `0x2898`
(8 count + 8 header + 32 key + 32 owner + 8 lamports + 8 data_len + 40 data +
10240 realloc padding + 8 rent_epoch + 8 ix-len). Cross-checked against a prior
graduate's public solution: identical offsets. Success path: exactly 4 CUs.

## The program

```asm
.equ CLOCK_SLOT, 0x0060       // r1 + 0x60   -> u64 current slot (Clock data)
.equ MAX_SLOT_HEIGHT, 0x2898  // r1 + 0x2898 -> u64 caller-supplied deadline

.globl entrypoint
entrypoint:
    ldxdw r2, [r1+MAX_SLOT_HEIGHT]  // deadline from instruction data
    ldxdw r1, [r1+CLOCK_SLOT]       // current slot from Clock account data
    jle r1, r2, end                 // current <= deadline: exit 0
    lddw r0, 1                      // deadline missed
end:
    exit
```

The happy path never touches `r0` — the VM zero-initializes it, and that free `0` is
what keeps the success path at 4 instructions. Disassembly of the shipped `.so`
round-trips to exactly these 5 instructions.

## Lessons

1. **A test harness tells you what your code does; only the integration target tells
   you what it must do.** Mollusk found the r0-clobber in milliseconds; only the live
   verifier could reveal that the documented contract wasn't the tested one.
2. **CU counts are fingerprints.** "3 consumed" identified which branch fired;
   "max 4" identified the entire intended solution.
3. If you use this pattern in production with untrusted callers, validate the account
   key — the 4-CU version trusts the caller to pass the real Clock sysvar.

## Tests

Each program revision was driven test-first (red before green); the final suite
models the verifier's actual contract:

| Test | What it pins down |
|------|-------------------|
| `passes_when_current_slot_below_deadline` | happy path |
| `passes_when_current_slot_equals_deadline` | boundary — `jle` is inclusive |
| `fails_when_current_slot_exceeds_deadline` | exact error: `ProgramError::Custom(1)` |
| `passes_with_max_u64_deadline` | no overflow at `u64::MAX` |
| `fails_when_max_slot_is_zero_and_current_nonzero` | degenerate deadline |
| `cu_budget_in_success_path` | regression guard: ≤ 4 CUs (verifier cap) |
| `cu_budget_in_failure_path` | regression guard: ≤ 5 CUs |

## Reproduce

```sh
cargo install --git https://github.com/blueshift-gg/sbpf.git
sbpf build        # ~2 ms
cargo test        # 7/7 via Mollusk (Agave compute model)
```

## Stack

- [sbpf](https://github.com/blueshift-gg/sbpf) — assembler, scaffold, debugger by [@deanmlittle](https://github.com/deanmlittle) & Blueshift
- [Mollusk](https://github.com/buffalojoec/mollusk) — SVM test harness
- [Blueshift](https://learn.blueshift.gg) — the challenge and the assembly course

## License

MIT
