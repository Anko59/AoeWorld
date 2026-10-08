//! Agents do not program the shell: no state-changing builtins, arithmetic,
//! assigning expansions or standalone assignments. The main session keeps
//! its own rules.
use super::{super::role::Role, Fixture, allowed, denied};

const AGENTS: [Role; 4] = [Role::Tester, Role::Implementer, Role::Reviewer, Role::Other];

#[test]
fn agent_variable_destinations_must_be_literal_identifiers() {
    let fixture = Fixture::new();
    let attacks = [
        "name=GNUMAKEFLAGS; export \"$name\"; read \"$name\" <<< '-i'; make lint",
        "target=GNUMAKEFLAGS; declare -n ref=\"$target\"; ref=-i make preflight",
        "target=PATH; printf -v $target '%s' /tmp/agent-bin; make lint",
        "name=PATH; export \"$name\"; printf -v \"$name\" '%s' '.agent-bin:/usr/bin:/bin'; git status",
        "target=GNUMAKEFLAGS; printf -v \"$target\" '%s' '--eval=probe:$(shell touch /tmp/pwn)'; export \"$target\"; make help",
    ];
    for role in AGENTS {
        for command in attacks {
            denied(&fixture, role, command);
        }
        for command in [
            "export \"$name\"",
            "declare \"$name\"",
            "typeset \"$name\"",
            "local \"$name\"",
            "readonly \"$name\"",
            "unset \"$name\"",
            "read \"$name\"",
            "read -a \"$name\"",
            "printf -v \"$name\" '%s' x",
            "printf \"$option\" \"$name\" '%s' x",
            "mapfile \"$name\" < file",
            "mapfile -n 1 \"$name\" < file",
            "readarray \"$name\" < file",
            "readarray -O 0 \"$name\" < file",
            "getopts x \"$name\" arg",
            "for \"$name\" in x; do true; done",
            "select \"$name\" in x; do true; done",
            "let '$name=1'",
            "let \"$name=1\"",
            "(( $name=1 ))",
        ] {
            denied(&fixture, role, command);
        }
        // These literal, unprotected forms were allowed before; agents no
        // longer change shell state at all.
        for command in [
            "export output",
            "declare output=x",
            "typeset output=x",
            "local output",
            "readonly output",
            "read output < file",
            "read -a output < file",
            "printf -v output '%s' x",
            "mapfile output < file",
            "readarray output < file",
            "getopts x output arg",
            "let 'output=1'",
            "(( output=1 ))",
        ] {
            denied(&fixture, role, command);
        }
        allowed(&fixture, role, "for output in x; do true; done");
    }
}

#[test]
fn agents_never_use_select() {
    // select reads a menu choice from standard input: refused outright.
    let fixture = Fixture::new();
    for role in AGENTS {
        denied(&fixture, role, "select output in x; do true; done");
    }
}

#[test]
fn agents_refuse_namerefs_and_protected_literal_builtin_destinations() {
    let fixture = Fixture::new();
    for role in AGENTS {
        for command in [
            "declare -n ref=output",
            "typeset -nr ref=output",
            "local -n ref=output",
            "export GNUMAKEFLAGS",
            "declare -x GNUMAKEFLAGS",
            "typeset GNUMAKEFLAGS=value",
            "local GNUMAKEFLAGS",
            "readonly GNUMAKEFLAGS",
            "unset -n GNUMAKEFLAGS",
            "read -a GNUMAKEFLAGS",
            "printf -v GNUMAKEFLAGS '%s' -i",
            "mapfile GNUMAKEFLAGS < file",
            "readarray GNUMAKEFLAGS < file",
            "getopts x GNUMAKEFLAGS arg",
            "for GNUMAKEFLAGS in x; do true; done",
            "for PATH in /tmp/agent-bin; do make lint; done",
            "select GNUMAKEFLAGS in x; do true; done",
            "let 'GNUMAKEFLAGS=1'",
            "(( GNUMAKEFLAGS=1 ))",
            "export PATH",
            "read PATH",
            "printf -v PATH '%s' /tmp/agent-bin",
        ] {
            denied(&fixture, role, command);
        }
        for command in ["eval 'true'", "source env.sh", ". env.sh"] {
            denied(&fixture, role, command);
        }
    }
}

#[test]
fn agents_refuse_every_reviewed_assignment_bypass() {
    let fixture = Fixture::new();
    for role in AGENTS {
        for command in [
            // F1: `:=` inside an arithmetic expansion, exported by `set -a`.
            "set -a; printf '%s' \"$(( ${GNUMAKEFLAGS:=-i} ))\"; make preflight",
            "echo \"$(( ${GNUMAKEFLAGS:=-i} ))\"",
            "echo ${GNUMAKEFLAGS:=-i}",
            "echo ${GNUMAKEFLAGS=-i}",
            "echo \"${X:=}\"",
            "echo ${a[i++]}",
            "echo ${x:-$(git push origin dev)}",
            "echo $(( x = 1 ))",
            "echo $[ x = 1 ]",
            "for (( i = 0; i < 2; i++ )); do true; done",
            "cat <<EOF\n${GNUMAKEFLAGS:=-i}\nEOF",
            // F2, F3: `%n` assigns the count to the argument's name.
            "name=PATH; printf '%n' \"$name\"",
            "target=PATH; printf 'x%n' \"$target\"",
            "printf '%n' PATH",
            "printf -- 'x%5n' PATH",
            "printf 'x%ln' PATH",
            // F5: attached array name.
            "read -aPATH <<< '/tmp/agent-bin'; make lint",
            // F6: computed option.
            "option=n; declare -\"$option\" ref=GNUMAKEFLAGS; ref=-i make lint",
            // F7: attached `-v` argument.
            "printf -vPATH '%s' '/tmp/agent-bin:/usr/bin:/bin'; make lint",
            "printf -v output x",
            "printf -- -voutput x",
            // State-changing builtins and keywords.
            "set -a",
            "set -o allexport",
            "set -e",
            "shopt -s expand_aliases",
            "hash -p /tmp/agent-bin/make make",
            "alias make=true",
            "trap 'git push' EXIT",
            "f() { make lint; }; f",
            "function f { true; }",
            "coproc true",
            "wait -p PATH",
            "builtin export PATH",
            "command declare -x GNUMAKEFLAGS=-i",
            "true {PATH}>/dev/null",
            // Assignments that persist in the shell.
            "output=1",
            "PATH=/tmp/agent-bin; make lint",
            "if true; then PATH=/tmp/agent-bin; fi; make lint",
            "{ GNUMAKEFLAGS=-i; }; make lint",
            "time GNUMAKEFLAGS=-i",
            "true && output=1",
            "arr=(x y)",
        ] {
            denied(&fixture, role, command);
        }
    }
}

#[test]
fn agents_keep_ordinary_commands_and_prefix_assignments() {
    let fixture = Fixture::new();
    for role in AGENTS {
        for command in [
            "make lint",
            "FOO=1 make lint",
            "FOO=1 BAR=x git status",
            "if true; then FOO=1 make lint; fi",
            "git status && git diff --stat && git log --oneline -3",
            "grep -rn 'fn main' crates",
            "echo '(( X=1 ))'",
            "echo '$(( X=1 ))' \"literal ((\"",
            "echo '${X:=1}' '%n'",
            "printf '%s\\n' x",
            "printf -- '%s %d%%\\n' x 5",
            "printf 'task\\n\\nbody\\n' | make issue",
            "echo $HOME ${HOME} $1 ${9} $? ${?} $$ $# $@ \"$*\" ${@}",
            "echo \"$HOME\"",
            "git diff",
            "for output in a b; do echo \"$output\"; done",
            "wait",
            "cd crates && ls",
        ] {
            if matches!(role, Role::Reviewer | Role::Other) && command.contains("make issue") {
                continue;
            }
            allowed(&fixture, role, command);
        }
    }
}

#[test]
fn agents_expand_only_plain_parameters() {
    let fixture = Fixture::new();
    for role in AGENTS {
        for command in [
            // F1: `@P` prompt expansion runs command substitutions.
            "for PS1 in '$(echo x > gates/registry.json)'; do echo \"${PS1@P}\"; done",
            // F3: a process substitution inside a default value.
            "echo ${__AOE_REVIEW_UNSET:-<(touch /tmp/pwn)}",
            // F5: an unquoted body joins `$\` and `((PATH=0))`.
            "cat <<EOF\n$\\\n((PATH=0))\nEOF\nmake lint",
            // F2, F4: `exec` in every form.
            "exec > agent-output.log; echo result",
            "exec 3>/tmp/agent-out; echo x >&3",
            "exec make lint",
            "exec -a name git status",
            "command exec git status",
            // Every other parameter form. `${#x}` and `${x:-default}` were
            // allowed before; agents now expand only the plain forms.
            "echo $HOME ${HOME} ${#x} ${x:-default}",
            "echo ${#x}",
            "echo ${x:-default}",
            "echo \"${PS1@P}\"",
            "echo ${x@E} ${x@Q}",
            "echo ${!x}",
            "echo ${!prefix*}",
            "echo ${x:1:2}",
            "echo ${x#a} ${x%b} ${x/a/b} ${x^^}",
            "echo ${x-y}",
            "echo ${10} ${0} $0 $! $- ${!}",
            "echo \"${a[0]}\"",
        ] {
            denied(&fixture, role, command);
        }
    }
}

#[test]
fn agents_use_no_here_documents_here_strings_or_continuations() {
    let fixture = Fixture::new();
    for role in AGENTS {
        for command in [
            // Quoted here-documents were allowed on the split branch; agents
            // now write files with their editor tools.
            "cat <<'EOF'\nplain $(not run) ${x@P}\nEOF",
            "cat <<\"EOF\"\n${X:=1}\nEOF\nmake lint",
            "cat <<-'EOF'\n\tbody\n\tEOF",
            "cat <<'EOF'\n${X:=1} $(( X=1 ))\nEOF",
            "cat <<EOF\nplain\nEOF",
            "cat <<EOF > /tmp/x\nplain\nEOF",
            "cat <<-EOF\n\tplain\n\tEOF",
            "cat <<\"$x\"\n$x\ntouch gates/registry.json\n\nEOF",
            "cat <<'EOF' $(true\n)\nEOF\ntouch gates/registry.json",
            "cat<<EOF\nx\nEOF",
            "cat 0<<EOF\nx\nEOF",
            "cat <<\\EOF\n$(touch gates/registry.json)\nEOF",
            // F1 and F4: a backslash-newline inside the delimiter.
            "cat <<E\\\nOF\n$(touch gates/registry.json)\nEOF\nOF",
            "cat <<EOF\\\n\n$(touch gates/registry.json)\nEOF",
            // F2: a quoted here-document inside a command substitution.
            "echo \"$(cat <<'EOF'\nbody\nEOF\n)\"",
            "echo `cat <<'EOF'\nbody\nEOF\n`",
            "bash -c \"cat <<'EOF'\nbody\nEOF\"",
            // Here-strings.
            "cat <<< x",
            "cat <<<'x'",
            "grep -c x <<< \"$HOME\"",
            // Backslash-newline continuations, in every quoting context.
            "make \\\nlint",
            "git status && \\\n git diff",
            "echo \"a\\\nb\"",
            "echo $(true \\\n)",
            "echo `true \\\n`",
            "echo x\\\n$(touch gates/registry.json)",
        ] {
            denied(&fixture, role, command);
        }
    }
}

#[test]
fn the_main_session_keeps_its_shell() {
    let fixture = Fixture::new();
    for command in [
        "export output=1",
        "declare -n ref=output",
        "read output < file",
        "printf -v output '%s' x",
        "set -euo pipefail",
        "(( i = 1 ))",
        "echo $(( 1 + 2 )) ${x:=1}",
        "output=1",
        "f() { true; }; f",
        "eval 'true'",
        "exec > output.log",
        "echo ${x@Q} ${!x} ${x:-default}",
        "cat <<EOF\n$HOME\nEOF",
        "cat <<'EOF'\nbody\nEOF",
        "cat <<-EOF\n\tbody\n\tEOF",
        "cat <<< x",
        "make \\\nlint",
        "echo $[ 1 + 2 ] {fd}>/dev/null",
        // F3 and F5: the protected Make variables bind agents only, so the
        // main session's prefix assignments keep their verdicts.
        "SHELL=/bin/bash make lint",
        "TOOL_IMAGE=custom make lint",
        "GNUMAKEFLAGS=-k make lint",
        "make GNUMAKEFLAGS=-k lint",
        "make MFLAGS=-k lint",
    ] {
        allowed(&fixture, Role::Main, command);
    }
    // These were refused to the main session before and still are.
    for command in ["make SHELL=/bin/bash lint", "make TOOL_IMAGE=custom lint"] {
        denied(&fixture, Role::Main, command);
    }
}

#[test]
fn agents_keep_protected_make_variables() {
    let fixture = Fixture::new();
    for role in AGENTS {
        for command in [
            "SHELL=/bin/bash make lint",
            "TOOL_IMAGE=custom make lint",
            "GNUMAKEFLAGS=-i make lint",
            "MFLAGS=-i make lint",
            "make GNUMAKEFLAGS=-i lint",
            "make MFLAGS=-i lint",
        ] {
            denied(&fixture, role, command);
        }
    }
}
