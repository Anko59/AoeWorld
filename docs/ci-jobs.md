# What a CI job prepares

Only `dev-artifacts` runs `make bootstrap`. Every other job runs its goal, and each goal builds the
images it needs as prerequisites. `make bootstrap` also pulls the browser image, installs the npm
packages and installs git hooks, which no `static` leg or performance job uses: it cost 163 to 242 s
in nine jobs of every run (measurements: [#1126](https://github.com/Anko59/AoeWorld/pull/1126)).

A Make goal that runs in an image names that image as a prerequisite (`perf-dev-reference`,
`perf-comment`), so it also works as the first command of a job.

`make perf-comment` is the last step of `target-performance` on pull requests, not a job: it needs
the harness and the orchestrator image that job already built, and it ran 17 s inside a 230 s job.
The step is informational (`continue-on-error`), and only the job's cache save, if it has one, may
follow it. The job has `pull-requests: write` for it and its checkout keeps no token in `.git/config`;
the `perf-dev-reference` step still gets the token in its environment, as the separate job's steps
did. The job table of the comment times the running job up to the moment the comment is built and
labels it `(failure)` when a step before the comment failed (`AOE_JOB_STATUS`, from `job.status`).
