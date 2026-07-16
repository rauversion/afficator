# P2P Sharing Foundation

Rau Connect is the foundation for peer identity, contacts, presence, shared-folder catalogs, chat, and peer-to-peer downloads. The local security and catalog boundaries remain separate from the transport, while authenticated Iroh protocols now carry diagnostics, catalog searches, file bytes, and chat messages between running instances.

## Current Scope

Implemented:

- one Ed25519 device identity;
- password-based identity unlocking;
- Argon2id key derivation with versioned parameters;
- AES-256-GCM private-key encryption inside SQLite;
- in-memory unlocked identity that is cleared when locked or when the process exits;
- persistence tables for trusted peers and presence state;
- read-only shared-folder definitions;
- recursive catalog indexing that ignores hidden entries and symlinks;
- virtual relative paths that do not expose the local absolute path to peers;
- stable opaque file IDs scoped to each share;
- local catalog search using the same response shape intended for remote queries;
- pause, reindex, and remove operations that never modify original files;
- Iroh endpoint lifecycle backed by the decrypted device identity;
- direct QUIC connectivity with relay fallback through the Iroh N0 preset;
- versioned `/rau/diagnostic/1` request/response traffic;
- shareable endpoint tickets, authenticated peer IDs, and measured round-trip time;
- observed peer persistence and a two-minute recent-reachability presence lease;
- return-ticket exchange so either peer can initiate later traffic;
- bounded and authorized `/rau/catalog/1` remote searches;
- streamed `/rau/file/1` downloads with temporary files, progress events, and safe replacement;
- path, symlink, share state, and peer authorization revalidation before every download;
- persisted private and general `/rau/chat/1` messages with delivery acknowledgements;
- Tauri network, transfer, and chat events plus React controls for the complete flow.

Not yet implemented:

- QR rendering, one-use pairing invitations, and contact authorization;
- periodic presence heartbeats;
- selected-contact ACLs and explicit trust promotion;
- content hashes and resumable downloads;
- message deletion, unread counters, and offline delivery;
- community discovery and moderation.

## Identity Storage

The device generates a 32-byte Ed25519 seed. Its public key becomes the stable endpoint ID. The private seed is never stored as plaintext.

```text
password + random salt
        |
        v
Argon2id
        |
        v
256-bit wrapping key
        |
        v
AES-256-GCM(device private seed, endpoint ID as AAD)
        |
        v
SQLite p2p_identity
```

The endpoint ID is included as authenticated associated data. Moving ciphertext to a different endpoint record therefore fails authentication. The salt, nonce, KDF parameters, and cipher version are non-secret and are stored alongside the ciphertext.

Rau cannot recover a forgotten P2P password. A future recovery/export flow must explicitly re-encrypt the identity with a recovery key or replacement password.

## Shared Folder Boundary

A share grants catalog and download access to a virtual read-only root. It does not grant arbitrary filesystem access.

```text
Local path:    /Users/alicia/Music/Masters/House/Night Drive.aiff
Share root:    /Users/alicia/Music/Masters
Remote path:   House/Night Drive.aiff
Remote file:   SHA256(share_id || 0x00 || remote_path)
```

The catalog excludes hidden paths and symlinks. Before each download, the backend resolves the selected file again, rejects absolute paths and non-normal path components, checks every component for symlinks, verifies that the canonical result remains beneath the share root, confirms the share is enabled, and re-checks peer authorization. Immutable content hashes remain a future integrity and resume boundary.

## SQLite Tables

- `p2p_identity`: encrypted device identity and versioned KDF parameters.
- `p2p_peers`: paired endpoint IDs, trust, last address, and last presence observation.
- `p2p_shares`: local roots, visibility policy, counters, and enabled state.
- `p2p_shared_files`: opaque IDs and virtual metadata for indexed files.
- `p2p_chat_messages`: incoming/outgoing private or general messages and delivery state.

Visibility values are intentionally bounded:

- `contacts`
- `selected_contacts`
- `community`
- `ticket`

`selected_contacts` will be backed by a share ACL table when pairing is implemented. `ticket` will be backed by hashed, expiring capability tokens.

## Tauri Commands

Identity:

- `p2p_identity_status`
- `p2p_create_identity`
- `p2p_unlock_identity`
- `p2p_lock_identity`

Catalog:

- `p2p_list_shares`
- `p2p_add_share`
- `p2p_reindex_share`
- `p2p_set_share_enabled`
- `p2p_remove_share`
- `p2p_search_shared_files`
- `p2p_remote_search`
- `p2p_download_remote_file`

Chat:

- `p2p_chat_list`
- `p2p_chat_send`

Peers:

- `p2p_list_peers`

Network:

- `p2p_network_status`
- `p2p_network_start`
- `p2p_network_stop`
- `p2p_network_ping_ticket`

## Network Handshake

The current network flow proves transport and identity before catalog or chat permissions are added:

```text
Device A ticket
      |
      v
Iroh connect(ALPN /rau/diagnostic/1)
      |
      +-- QUIC authenticates Device B endpoint ID
      |
      v
bounded JSON ping(nonce, version, public display name, return ticket)
      |
      v
bounded JSON pong(same nonce, B endpoint ID, display name)
      |
      v
A verifies pong endpoint ID == authenticated QUIC peer ID
      |
      v
peer observation + validated return ticket + RTT + p2p-network-event
```

The endpoint ticket is public connection metadata, not the private key. The future QR screen will encode this ticket together with a short-lived, one-use invitation capability. Scanning a raw diagnostic ticket currently proves which endpoint answered and records it as an observed contact, but does not yet promote it through an explicit trust ceremony.

`online` currently means that the peer completed authenticated diagnostic, catalog, download, or chat traffic during the last two minutes. It automatically appears `offline` after the lease expires. Periodic heartbeat traffic will renew this lease in a later slice.

## Service Protocols

- `/rau/catalog/1`: bounded JSON query and virtual metadata response. Known, non-blocked peers can see enabled `contacts`, `community`, and `ticket` shares. `selected_contacts` is denied until its ACL exists.
- `/rau/file/1`: bounded request, length-prefixed metadata header, then streamed file bytes. The receiver writes to a temporary sibling and only replaces the selected destination after the byte count is complete.
- `/rau/chat/1`: bounded private/general message and authenticated delivery acknowledgement. Messages travel inside Iroh's encrypted transport and are stored as plaintext in the local SQLite database.

The current general room is a direct fan-out to every known peer with a return ticket. It is deliberately not a globally discoverable public room.

## Next Network Slice

The next slice can build on the verified transport without changing the local catalog shapes:

1. Add `/rau/pair/1`, expiring one-use invitations, QR rendering, and explicit acceptance.
2. Promote accepted endpoint IDs from `observed` to `paired` in `p2p_peers`.
3. Add periodic presence heartbeats for paired contacts.
4. Add immutable content hashes and resumable file ranges.
5. Add unread state, retry queues, and offline chat delivery.
6. Add an optional encrypted-at-rest chat database policy.
7. Design community discovery and moderation as a separate trust boundary.

The public general room should remain a separate protocol and policy boundary from trusted private contacts.
