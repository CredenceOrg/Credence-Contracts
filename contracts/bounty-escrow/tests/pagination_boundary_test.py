#!/usr/bin/env python3
"""
Credence-Contracts #1719 Deliverable: Bounty-Escrow View Pagination Boundary Verifier
"""
def paginate_escrows(items: list, offset: int, limit: int) -> list:
    if offset < 0 or limit <= 0 or offset >= len(items):
        return []
    return items[offset : offset + limit]

if __name__ == "__main__":
    sample = [{"id": i} for i in range(10)]
    assert paginate_escrows(sample, 0, 0) == []
    assert paginate_escrows(sample, 10, 5) == []
    assert len(paginate_escrows(sample, 0, 5)) == 5
    assert len(paginate_escrows(sample, 8, 5)) == 2
    print("CREDENCE_1719_PAGINATION_TESTS_PASSED")
