# Original AoeWorld artwork

Optimized copies of the owner-provided generated art collection. No original game assets.
Source root: `/home/anko/.codex/worktrees/visual-identity/AoeWorld/local-assets/art/`.
The source collection is read only. Paintings keep their native width, below 1920 pixels.

| Committed file | Source relative to the collection |
| --- | --- |
| `royal-archive.webp` | `series/01-kings-with-globes/01-franks.png` |
| `lakeside-citadel.webp` | `series/17-castles-walls/01-lithuanians.png` |
| `homecoming.webp` | `series/11-victory/01-franks.png` |
| `wordmark.webp` | `design-explorations/03-logo-wordmarks/01-classic-incised-gold.png` |
| `favicon.ico`, `icon-180.png`, `icon-512.png` | `design-explorations/02-favicons/01-bronze-globe.png` |

Logo and icons preserve the sources’ existing transparency and are cropped to their visible alpha bounds, ignoring near-transparent background flecks.
The favicon ICO contains sixteen, thirty-two and forty-eight pixel images.

## Animated opening

Four silent image-to-video clips generated on 2026-10-14 with Alibaba Wan 3.0
(first-frame image-to-video) through OpenRouter, from our own collection.
Each plays once: three seconds, 1280 × 720, VP9 WebM (CRF 40), without audio.
JPEG posters are their first frames and provide the still-image fallback.
Generation cost: USD 1.91 including a two-second test, recorded in the source ledger.

| Clip and matching poster in `cinematic/` | Collection source |
| --- | --- |
| `shot1-king.webm`, `shot1-king-poster.jpg` | `series/01-kings-with-globes/01-franks.png` |
| `shot2-ship.webm`, `shot2-ship-poster.jpg` | `series/15-maritime-war/01-byzantines.png` |
| `shot3-cavalry.webm`, `shot3-cavalry-poster.jpg` | `series/20-cavalry-charge/01-spanish.png` |
| `shot4-castle.webm`, `shot4-castle-poster.jpg` | `series/17-castles-walls/03-britons.png` |

The clips and posters total 4,583,579 bytes; all committed art is 5,961,323 bytes
(excluding this provenance file). Unused intro-only paintings were removed.
