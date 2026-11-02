# Voice-over for a recorded video

A busy live scene often cannot go through `make showcase` (settle rule, the
60 s plan cap that counts load time), so a Playwright script and an ffmpeg cut
make the video, which is silent. `make showcase-narrate` voices it:

```
make showcase-narrate NARRATE_VIDEO=<in.webm> NARRATE_LINES=<lines.json> \
  SHOWCASE_OUT=.cache/showcase/<out>.webm   # NARRATE_VOICE=Kore by default
```

`lines.json` is `[{"at": 0, "text": "..."}, {"at": 9.5, "text": "..."}]`: 1 to 40
lines in ascending `at` order, `at` from 0 to 300 seconds, 1 to 3000 characters
each, no other keys. Each line is spoken with the narration path above (same
model, cache and keyring credential) and starts at its `at`. A line that would
overlap the previous one, or speak past the end of the video, is refused, as is
one starting at or after the end; no speech is requested for a late `at`. Leave
a second between lines and estimate about 15 characters per second of speech.
The video needs a recorded duration (a cut by ffmpeg has one; otherwise remux
with `ffmpeg -i in -c copy out.webm`). The video stream is copied untouched, any
existing audio is replaced, and the result is written like `make showcase`
output (inside `.cache/showcase/`), with one opus track of real speech and
silence between lines, so `make describe-check` counts it as voiced.
Code: `crates/harness/src/ship/showcase/voiceover/`.

