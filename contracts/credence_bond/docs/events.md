# Credence Bond Event Emissions

This document details the event emission system for the Credence Bond smart contract. The event architecture is designed to allow off-chain indexers and client applications to easily track the lifecycle of identity bonds, enabling efficient filtering and state reconstruction.

## Event Architecture & Filtering

To support efficient indexing, all bond lifecycle events follow a strict, standardized topic signature:

* **Topic 0 (Routing):** A `Symbol` representing the specific event name (e.g., `"bond_created"`).
* **Topic 1 (Filtering):** The `Address` of the identity that owns the bond.

**Why this matters:** Indexers can listen to the contract ID and filter strictly by Topic 1 (`identity`) to retrieve the entire historical event log for a specific user in $O(1)$ time, without needing to parse the data payloads of unrelated users.

---

## Event Dictionary

### 1. `bond_created`
Emitted when an identity successfully opens a new bond.

* **Topics:** * `[0]` `Symbol`: `"bond_created"`
    * `[1]` `Address`: `identity` (The owner of the bond)
* **Data Payload:** `(i128, u64, bool)`
    * `0`: `amount` (`i128`) - The initial number of tokens bonded.
    * `1`: `duration` (`u64`) - The lock-up period for the bond in seconds.
    * `2`: `is_rolling` (`bool`) - Flag indicating if the bond auto-renews at the end of its duration.

### 2. `bond_increased`
Emitted when an identity tops up an existing active bond with additional funds.

* **Topics:**
    * `[0]` `Symbol`: `"bond_increased"`
    * `[1]` `Address`: `identity` (The owner of the bond)
* **Data Payload:** `(i128, i128)`
    * `0`: `added_amount` (`i128`) - The specific amount of tokens added in this transaction.
    * `1`: `new_total` (`i128`) - The new, total bonded amount for the identity.

### 3. `bond_withdrawn`
Emitted when funds are withdrawn from a bond. This applies to standard post-lockup withdrawals, early exits (with penalties applied), and full bond closures. 

* **Topics:**
    * `[0]` `Symbol`: `"bond_withdrawn"`
    * `[1]` `Address`: `identity` (The owner of the bond)
* **Data Payload:** `(i128, i128)`
    * `0`: `amount_withdrawn` (`i128`) - The amount of tokens successfully withdrawn to the user's wallet.
    * `1`: `remaining_balance` (`i128`) - The amount of tokens left in the bond after the withdrawal (will be `0` for full withdrawals).

### 4. `bond_slashed`
Emitted when an authorized protocol administrator penalizes a bond, reducing its available value.

* **Topics:**
    * `[0]` `Symbol`: `"bond_slashed"`
    * `[1]` `Address`: `identity` (The owner of the penalized bond)
* **Data Payload:** `(i128, i128)`
    * `0`: `slash_amount` (`i128`) - The specific amount penalized in this current transaction.
    * `1`: `total_slashed` (`i128`) - The aggregate total of all slashes applied to this bond over its lifetime.