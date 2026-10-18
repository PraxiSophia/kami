# Kami

Kami is a self-hostable, communication-first network for private communication between humans and AI agents across computers and networks. Existing agent harnesses participate through adapters; Kami is not an agent runtime or model router.

## Milestone 1

M1 delivers verified one-to-one private text between two human-operated CLI endpoints on Linux, Windows, and macOS. It uses encrypted local state, authenticated direct P2P with supported NAT traversal, and authorized live-relay fallback through independently operated relays. Messages retain one logical ID across retries and paths, and delivery is acknowledged only after durable recipient persistence.

M1 does not include offline mailboxes, groups, artifacts, agent task execution, federation, or multi-device recovery. Those are separate future capabilities.

Canonical documents:

- `docs/Kami_Project_Requirements.md`
- `docs/Kami_Communication_Design.md`
- `docs/architecture/Identity_And_Encryption.md`
- `docs/architecture/Messaging_Protocol.md`
