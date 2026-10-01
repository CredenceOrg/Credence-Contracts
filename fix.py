import re

# 1. lib.rs
lib = open('contracts/credence_errors/src/lib.rs').read()

lib = lib.replace('NoPendingAdmin = 118,', 'NoPendingAdmin = 115,')

if 'InvalidStringifiedBytes = 230' not in lib:
    lib = lib.replace(
        'EmptyBatch = 228,\n',
        'EmptyBatch = 228,\n\n    /// A string expected to contain hex or base64 encoded bytes is malformed\n    /// or exceeds the maximum accepted encoded length.\n    /// Contracts: bond\n    /// Wire-stable: do not renumber this error code.\n    InvalidStringifiedBytes = 230,\n'
    )

if 'ContractError::InvalidStringifiedBytes => ErrorCategory::Bond' not in lib:
    lib = lib.replace(
        '| ContractError::EmptyBatch\n            | ContractError::DuplicateIdempotencyKey',
        '| ContractError::EmptyBatch\n            | ContractError::InvalidStringifiedBytes\n            | ContractError::DuplicateIdempotencyKey'
    )

if '"String is not valid bounded hex or base64 encoded bytes"' not in lib:
    lib = lib.replace(
        'ContractError::EmptyBatch => "Batch input is empty; at least one item is required",\n',
        'ContractError::EmptyBatch => "Batch input is empty; at least one item is required",\n            ContractError::InvalidStringifiedBytes => {\n                "String is not valid bounded hex or base64 encoded bytes"\n            }\n'
    )

if 'ContractError::InvalidStringifiedBytes // correct the encoded input' not in lib:
    lib = lib.replace(
        '| ContractError::EmptyBatch            // supply at least one item\n            | ContractError::AmountExplicitlyZero  // supply a non-zero amount\n            => true,',
        '| ContractError::EmptyBatch            // supply at least one item\n            | ContractError::AmountExplicitlyZero  // supply a non-zero amount\n            | ContractError::InvalidStringifiedBytes // correct the encoded input\n            => true,'
    )
open('contracts/credence_errors/src/lib.rs', 'w').write(lib)

# 2. test_errors.rs
test = open('contracts/credence_errors/src/test_errors.rs').read()

if 'ContractError::BorrowFrozen' not in test.split('fn all_variants()')[1].split(']')[0]:
    test = test.replace(
        'ContractError::ContractPaused,\n            ContractError::InvalidPauseAction',
        'ContractError::ContractPaused,\n            ContractError::BorrowFrozen,\n            ContractError::InvalidPauseAction'
    )

if 'ContractError::PromiseNotKept' not in test.split('fn all_variants()')[1].split(']')[0]:
    test = test.replace(
        'ContractError::PayloadTooOld,\n            ContractError::DomainMismatch',
        'ContractError::PayloadTooOld,\n            ContractError::PromiseNotKept,\n            ContractError::DomainMismatch'
    )

if 'ContractError::InvalidStringifiedBytes' not in test.split('fn all_variants()')[1].split(']')[0]:
    test = test.replace(
        'ContractError::EmptyBatch,\n            ContractError::DuplicateAttestation',
        'ContractError::EmptyBatch,\n            ContractError::InvalidStringifiedBytes,\n            ContractError::DuplicateAttestation'
    )

test = re.sub(r'(fn test_all_variants_count\(\) \{\s*assert_eq!\(\s*all_variants\(\)\.len\(\),\s*)(\d+)', r'\g<1>98', test)
test = re.sub(r'(assert_eq!\(\s*cases\.len\(\),\s*)(\d+)', r'\g<1>91', test)
open('contracts/credence_errors/src/test_errors.rs', 'w').write(test)

# 3. discriminant_uniqueness.rs
disc = open('contracts/credence_errors/tests/discriminant_uniqueness.rs').read()

if 'ContractError::BorrowFrozen' not in disc.split('ALL_VARIANTS')[1].split(';')[0]:
    disc = disc.replace(
        '("ContractPaused", ContractError::ContractPaused),\n    ("InvalidPauseAction", ContractError::InvalidPauseAction)',
        '("ContractPaused", ContractError::ContractPaused),\n    ("BorrowFrozen", ContractError::BorrowFrozen),\n    ("InvalidPauseAction", ContractError::InvalidPauseAction)'
    )

if 'ContractError::AmountExplicitlyZero' not in disc.split('ALL_VARIANTS')[1].split(';')[0]:
    disc = disc.replace(
        '("InvalidBondAmount", ContractError::InvalidBondAmount),\n    ("InvalidBondDuration", ContractError::InvalidBondDuration)',
        '("InvalidBondAmount", ContractError::InvalidBondAmount),\n    ("AmountExplicitlyZero", ContractError::AmountExplicitlyZero),\n    ("InvalidBondDuration", ContractError::InvalidBondDuration)'
    )

if 'ContractError::PayloadTooOld' not in disc.split('ALL_VARIANTS')[1].split(';')[0]:
    disc = disc.replace(
        '("DelegationNotExpired", ContractError::DelegationNotExpired),\n    ("DelegationInactive", ContractError::DelegationInactive)',
        '("DelegationNotExpired", ContractError::DelegationNotExpired),\n    ("PayloadTooOld", ContractError::PayloadTooOld),\n    ("DelegationInactive", ContractError::DelegationInactive)'
    )

if 'ContractError::PromiseNotKept' not in disc.split('ALL_VARIANTS')[1].split(';')[0]:
    disc = disc.replace(
        '("DelegationInactive", ContractError::DelegationInactive),\n    // --- Treasury (600-699) ---',
        '("DelegationInactive", ContractError::DelegationInactive),\n    ("PromiseNotKept", ContractError::PromiseNotKept),\n    // --- Treasury (600-699) ---'
    )

disc = re.sub(r'(const ALL_VARIANTS_COUNT:\s*usize\s*=\s*)(\d+)', r'\g<1>98', disc)
open('contracts/credence_errors/tests/discriminant_uniqueness.rs', 'w').write(disc)
print("done")
