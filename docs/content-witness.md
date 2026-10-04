# Logical content witnesses versus snapshot observations

A portable review subject must not be identified by incidental checkout paths or
physical Git storage. The scope checker reports an additional logical content
witness for immutable index and commit scopes. It is still non-authoritative.

Existing source/private snapshot fingerprints remain endpoint observations. Source
fingerprints bind the captured effective-index path, raw index/cache bytes and
workspace observations. Private fingerprints include the sealed physical Git
metadata representation. The metadata seal hashes relative Git paths and raw file
bytes: it does **not** directly serialize inode numbers, timestamps or absolute
private-checkout paths. Neither fingerprint API promises a portable logical review
identity; coincidentally equal fresh fingerprints do not change that contract.

## Logical identities

- A commit witness binds the selected full commit ID, its tree and the typed raw
  commit/tree/blob object contents reachable through that selected tree. It excludes
  an unrelated current workspace HEAD, index path/cache and Git representation.
- An index witness binds captured source HEAD, logical stage-zero path/mode/blob
  entries and actual raw blob contents. Its pending tree is generated in the private
  snapshot from those captured entries. A synthetic private commit is not a candidate
  commit, and the source repository need not contain that generated pending tree.
  The recorded source-HEAD commit bytes anchor the source capture only; they are
  not a claim that the private index export imports that commit or its history.
- File inventories preserve paths and executable modes, not merely a set of blobs.
  Renaming the same bytes or changing executable mode changes the witness.
- The selected tree closure is **not ancestor-history closure**. Commit parent IDs
  appear inside the selected raw commit, but parent object contents are not claimed
  as checked merely because the selected commit references them.

The canonical witness uses sorted typed records and an explicit domain-separated
BLAKE3 algorithm/version. This is a defined Rust serialization, not a claim to
implement a general canonical-JSON standard. Absolute paths, physical index bytes,
Git configuration, timestamps and durations belong outside this logical subject.

## Checks and limits

Witness construction retains the same immutable snapshot through its original
before/after source, private metadata and worktree checks. It also compares actual
source and private typed raw object BLAKE3 contents and worktree file bytes for the
selected inputs. Git object names alone are not the requested raw-byte proof.
Index witnesses compare logical captured entries/blobs with the private inventory;
source-HEAD trees cannot be substituted for the generated pending-tree closure.

These checks do not run tests, certify coverage, authenticate a reviewer, authorize
submission or qualify a service deployment. Candidate policy remains local feedback,
not protected judge policy. The coding UID's host-daemon access still prevents local
hashes, root-owned files or signatures from creating independent judge authority.
Local filesystem and Git operations are not hard wall-supervised, and endpoint
checks cannot detect every hostile edit reverted between observations.

The future exact-review adapter must generate a closed subject from these observed
logical witnesses plus the separate [contract digest](contracts.md), canonical
registry plans, complete affected paths and actual AST bindings. Snapshot-specific
observations must stay outside its reproducible comparator. Matching a submitted
subject will establish only exact byte/content agreement; independent review and
execution evidence remain unavailable until separately verified. That comparator
and authenticated verdict adaptation are not implemented by this witness alone.
