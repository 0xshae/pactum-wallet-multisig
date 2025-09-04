# Pactum: A Stateful Multisig Pallet for the Polkadot Ecosystem

A secure, wallet-first stateful multisig implementation for Substrate blockchains, inspired by Gnosis Safe.

Pactum introduces a stateful, on-chain multisignature wallet system that creates persistent, sovereign accounts on the blockchain. Unlike traditional stateless multisigs, Pactum's wallets are first-class citizens with their own account IDs, enabling native asset management and seamless integration with other pallets.

## 🚀 Quickstart

### Prerequisites

1. **Build the Node:**
   ```bash
   cargo build --release
   ```

2. **Run a Local Development Node:**
   ```bash
   ./target/release/substrate-node-template --dev --tmp
   ```
   - `--dev`: Runs in development mode with a fresh state on each start
   - `--tmp`: Uses a temporary database (cleared on restart)

3. **Run the Tests:**
   ```bash
   cargo test -p pallet-multisig
   ```

## 🏛️ Architecture & Design

Pactum implements a wallet-first stateful model where each multisig has its own sovereign account. This design enables:

- Native asset management without complex proxy patterns
- Clean integration with other pallets
- Improved user experience with deterministic account addresses

##  Design Considerations & Compromises

### Immutable Owners
The current implementation enforces immutable ownership for security and simplicity. This decision:
- Eliminates complex edge cases around dynamic ownership changes
- Prevents potential griefing attacks from malicious owner modifications
- Simplifies the security model and audit surface

A production implementation would need a governance mechanism to manage owners, which could be implemented as a future enhancement.

### Unbounded Storage Cleanup
The `destroyMultisig` function uses `clear_prefix` to clean up all related storage items. This choice:
- Ensures complete cleanup of all associated data
- Provides a clean slate for storage reclamation
- May have variable weight depending on the number of proposals

## Vision & Future Work

### 1: Compatibility with Polkadot JS App.


### 2: Production Hardening
1. **Wallet Governance**
   - Add/remove owners through governance proposals
   - Adjustable thresholds with appropriate cooldowns
   - Emergency recovery mechanisms

2. **Economic Security**
   - Proposal deposits to prevent spam
   - Slashing conditions for malicious behavior
   - Transaction fee management

3. **Call Filtering**
   - Allowlist/denylist for call destinations
   - Spending limits per time period
   - Permissioned call types

### 3: Ecosystem Integration
1. **Multi-Asset Support**
   - Native integration with pallet-assets
   - Cross-asset transaction batching
   - Asset-specific permissioning

2. **Cross-Chain Execution**
   - XCM integration for cross-chain proposals
   - Multi-chain governance
   - Bridge interactions

3. **Scheduled Transactions**
   - Integration with pallet-scheduler
   - Recurring payments
   - Time-locked transactions

## 🏛️ Architecture & Design

This pallet is built on a "wallet-first" stateful model, where each multisig is a persistent on-chain entity. This design choice, inspired by Gnosis Safe, allows the wallet to have its own sovereign `AccountId`, enabling it to hold funds and interact with other pallets as a first-class on-chain citizen.

The logic of the pallet is designed with a "fail fast" and "secure by default" philosophy. All extrinsics perform rigorous validation checks upfront before making any state changes.



### State Transition Function (STF): A Step-by-Step Logic Guide

The pallet's behavior is defined by its five core extrinsics. Here is a detailed breakdown of the implementation logic and the thought process behind each one.

---

#### 1. `create_multisig`

**Purpose:** To initialize a new, persistent multisig wallet and record its configuration on-chain.

**Step-by-Step Logic:**

1.  **Validation & Security:** The first priority is to validate the inputs before any state is written.
    * The function first calls `ensure_signed` to verify the transaction has a valid signature and to identify the creator.
    * Next, it converts the user-provided `owners` `Vec` into a `BoundedVec`. This is a critical safety measure to prevent a potential Denial-of-Service (DoS) attack where a user could submit a list with millions of owners, bloating storage and computation. If the number of owners exceeds the `MaxOwners` constant defined in the runtime, this conversion fails and the extrinsic exits with an error.
    * Finally, it performs a sanity check on the `threshold`, ensuring it's a logical value (greater than 0 and less than or equal to the number of owners).

2.  **State Changes:** Once validated, the function proceeds to create the on-chain records.
    * It fetches a new, unique `multisig_id` from the `NextMultisigId` storage counter. It then immediately increments and saves the counter using `checked_add` to prevent potential integer overflows.
    * It calls the `multi_account_id` helper function. This is the cornerstone of the stateful design, deterministically generating a unique, sovereign `AccountId` for the new wallet based on its ID.
    * It creates an instance of the `Multisig` struct and inserts it into the `Multisigs` storage map, officially bringing the wallet into existence on-chain.

3.  **Notification:** The function concludes by emitting a `MultisigCreated` event, broadcasting the `multisig_id` and, crucially, the wallet's new `multisig_account` address so that users can begin sending funds to it.

---

#### 2. `submit_proposal`

**Purpose:** To allow an authorized owner to formally propose a transaction for the group to approve.

**Step-by-Step Logic:**

1.  **Validation & Security:**
    * It verifies the caller is a signed user.
    * It checks that the specified `multisig_id` corresponds to an existing wallet.
    * **Authorization:** It performs the core permission check, ensuring the caller's account is present in the multisig's `owners` list.

2.  **State Changes:**
    * It gets a new `proposal_index` from the counter dedicated to this specific multisig. Each wallet maintains its own proposal count.
    * **Design Rationale (Storage Optimization):** To avoid storing potentially large `RuntimeCall` data on-chain, the function calculates the `blake2_256` hash of the call. It then creates and stores a `Proposal` struct containing only this hash and an `executed` flag.
    * **Design Rationale (User Experience):** The submitter is automatically added as the first approval. This is a deliberate UX improvement to save the user from having to send a second, separate `confirm_proposal` transaction for their own proposal.

3.  **Notification:** It emits a `ProposalSubmitted` event, providing the `call_hash` so other owners can verify the proposed action off-chain before confirming.

---

#### 3. `confirm_proposal`

**Purpose:** To allow other owners to cast their vote of approval for a pending proposal.

**Step-by-Step Logic:**

1.  **Validation & Security:** This function has the most extensive set of "fail fast" checks to ensure the integrity of the voting process.
    * It first checks for a valid signature, an existing multisig, and the caller's ownership status.
    * It then verifies that the specified proposal actually exists and has not already been executed.
    * **Critical Security Check:** It checks if the caller's account is already in the `Approvals` list for this proposal. This is vital to prevent a single owner from voting multiple times and artificially meeting the threshold.

2.  **State Changes:** The logic follows a safe "read-modify-write" pattern.
    * It reads the current list of `Approvals` from storage.
    * It pushes the new approver's `AccountId` to this list.
    * It writes the updated list back to storage.

3.  **Notification:** It emits a `Confirmation` event, which signals to UIs that the proposal's approval count has increased.

---

#### 4. `execute_proposal`

**Purpose:** To dispatch a fully approved transaction from the multisig's sovereign account.

**Step-by-Step Logic:**

1.  **Validation & Security:**
    * It first checks that the proposal exists and has not been executed.
    * **Critical Security Check (`CallHashMismatch`):** It requires the user to submit the full `call` data again. The function then hashes this provided call and ensures it matches the `call_hash` stored on-chain when the proposal was created. This prevents any "bait-and-switch" attack where a different action could be executed than the one owners approved.
    * **Core Authorization Check:** It verifies that the number of approvals in storage is greater than or equal to the multisig's `threshold`.

2.  **State Changes:**
    * It dispatches the `call` using the multisig's derived sovereign `AccountId` as the `Signed` origin. This is the moment the multisig "acts" on the blockchain.
    * **Design Rationale (Self-Destruction Safety):** After the dispatch, it only updates the proposal's `executed` flag if two conditions are met: the dispatch was successful (`result.is_ok()`) AND the parent multisig still exists (`Multisigs::contains_key(multisig_id)`). This second check is a crucial safety feature to handle the specific edge case where the executed call was `destroy_multisig`, preventing the code from trying to write to storage that has just been deleted.

3.  **Notification:** It emits a `ProposalExecuted` event, which includes the `result` of the inner dispatched call. This tells users not only that the execution was attempted, but whether the inner call succeeded or failed.

---

#### 5. `destroy_multisig`

**Purpose:** To provide a secure mechanism for cleaning up a wallet and its associated storage.

**Step-by-Step Logic:**

1.  **Validation & Security:**
    * **Design Rationale (Sovereign Security Model):** The most important check is `ensure!(who == multisig_account, ...)`. This enforces a "self-governance" model. The extrinsic can only be successfully called if its origin is the multisig's *own* sovereign account. This means destruction is not a simple user action; it must be proposed, confirmed, and executed like any other high-stakes proposal.
    * **Design Rationale (Fund Safety):** The function includes a critical safety net: `ensure!(balance.is_zero(), ...)`. It checks that the multisig's on-chain balance is zero. This prevents the accidental and irreversible destruction of a wallet that still holds funds, forcing the owners to explicitly empty it first.

2.  **State Changes:**
    * It performs a complete cleanup of all storage items associated with the `multisig_id`. It uses `remove` for single-key maps and the efficient `clear_prefix` to wipe all proposals and approvals for that wallet.

3.  **Notification:** It emits a `MultisigDestroyed` event to confirm the successful cleanup.

