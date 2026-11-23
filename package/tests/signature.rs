//! Signatures are reconstructed from isolated harness records without invoking an interpreter.
#![allow(clippy::unwrap_used)]

use rusqlite::Connection;
use serde_json::{Value, json};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use tempfile::{Builder, TempDir};

struct Fixture(TempDir);

impl Fixture {
    fn new() -> Self {
        let fixture = Self(
            Builder::new()
                .prefix("gitmoji signature ")
                .tempdir()
                .unwrap(),
        );
        let git = |args: &[&str]| {
            let result = fixture
                .command("git")
                .args(args)
                .env("GIT_AUTHOR_DATE", "2020-01-01T00:00:00Z")
                .env("GIT_COMMITTER_DATE", "2020-01-01T00:00:00Z")
                .output()
                .unwrap();
            assert!(
                result.status.success(),
                "{}",
                String::from_utf8_lossy(&result.stderr)
            );
        };
        git(&["init", "-q"]);
        git(&[
            "-c",
            "user.name=Test",
            "-c",
            "user.email=test@example.invalid",
            "commit",
            "-q",
            "--allow-empty",
            "-m",
            "Recorded work",
            "-m",
            "_\nfixture-model (high)\nCodex (thread)",
            "-m",
            "_\nfixture-model (low)\nClaude Code (claude-id)",
            "-m",
            "_\nfixture-model (low)\nAntigravity (root)",
        ]);
        fixture
    }

    fn root(&self) -> &Path {
        self.0.path()
    }

    fn path(&self, path: &str) -> PathBuf {
        self.root().join(path)
    }

    fn command(&self, program: &str) -> Command {
        let mut command = Command::new(program);
        command
            .current_dir(self.root())
            .env("HOME", self.root())
            .env("USERPROFILE", self.root())
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .env("GIT_CONFIG_GLOBAL", self.path("empty-config"));
        for key in [
            "GIT_DIR",
            "GIT_WORK_TREE",
            "GIT_COMMON_DIR",
            "GIT_INDEX_FILE",
            "GIT_OBJECT_DIRECTORY",
            "GIT_ALTERNATE_OBJECT_DIRECTORIES",
            "GIT_CONFIG_COUNT",
            "GIT_CONFIG_PARAMETERS",
        ] {
            command.env_remove(key);
        }
        command
    }

    fn log(&self, path: &str, rows: &[Value]) {
        let path = self.path(path);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        let mut data = String::from("incomplete row\n");
        for row in rows {
            data.push_str(&row.to_string());
            data.push('\n');
        }
        fs::write(path, data).unwrap();
    }

    fn run(&self, args: &[&str], vars: &[(&str, &str)]) -> Output {
        let mut cmd = self.command(env!("CARGO_BIN_EXE_gitmoji"));
        for key in [
            "CLAUDECODE",
            "CLAUDE_CODE_ENTRYPOINT",
            "CLAUDE_CODE_SESSION_ID",
            "CLAUDE_EFFORT",
            "CODEX_THREAD_ID",
            "CODEX_SESSION_ID",
            "ANTIGRAVITY_CONVERSATION_ID",
        ] {
            cmd.env_remove(key);
        }
        cmd.arg("signature")
            .args(args)
            .envs(vars.iter().copied())
            .env("CODEX_HOME", self.path("codex"))
            .env("ANTIGRAVITY_APP_DATA_DIR", self.path("agy"))
            .output()
            .unwrap()
    }

    fn text(&self, args: &[&str], vars: &[(&str, &str)]) -> String {
        let out = self.run(args, vars);
        assert!(
            out.status.success(),
            "{}",
            String::from_utf8_lossy(&out.stderr)
        );
        String::from_utf8(out.stdout).unwrap()
    }
}

fn ctx(model: &str, effort: &str, turn: &str) -> Value {
    json!({"type":"turn_context","payload":{"model":model,"effort":effort,"turn_id":turn}})
}
fn event(thread: &str, stamp: &str, item: Value) -> Value {
    json!({"type":"event_msg","timestamp":stamp,"payload":{"type":"item_completed","thread_id":thread,"turn_id":"t","item":item}})
}
const BEFORE: &str = "2019-12-31T23:00:00Z";
const NOW: &str = "2020-01-01T00:00:01.123Z";

#[test]
fn repeated_review_links_are_git_trailers_for_humans_and_agents() {
    let f = Fixture::new();
    let refs = [
        "--review-thread",
        "https://work.ghe.com/team/app/pull/4#discussion_r7",
        "--review-thread",
        "https://code.example/review/42",
    ];
    let trailers = format!("Review-Thread: {}\nReview-Thread: {}\n", refs[1], refs[3]);
    assert_eq!(f.text(&refs, &[]), trailers);
    let signature = f.text(&refs, &[("CODEX_THREAD_ID", "thread-one")]);
    assert!(signature.starts_with("_\n") && signature.ends_with(&format!("\n\n{trailers}")));
    let file = f.path("message");
    fs::write(
        &file,
        format!("subject\n\nThe change has a reason.\n\n{signature}"),
    )
    .unwrap();
    let parsed = f
        .command("git")
        .args(["interpret-trailers", "--parse"])
        .arg(file)
        .output()
        .unwrap();
    assert!(parsed.status.success());
    assert_eq!(
        String::from_utf8(parsed.stdout)
            .unwrap()
            .replace("\r\n", "\n"),
        trailers
    );
    let bad = f.run(&["--review-thread", "#42"], &[]);
    assert!(!bad.status.success() && bad.stdout.is_empty());
    assert!(
        String::from_utf8(bad.stderr)
            .unwrap()
            .contains("one absolute HTTPS URL")
    );
}

#[test]
fn codex_uses_owned_rollouts_and_the_work_since_the_last_signed_commit() {
    let f = Fixture::new();
    let web = event(
        "thread",
        NOW,
        json!({"type":"WebSearch","id":"web","action":{"type":"search","queries":["one","two"]},
        "results":[{"ref_id":"turn0view1"},{"ref_id":"turn0view1"},{"ref_id":"turn0search9"}]}),
    );
    f.log("codex/sessions/2020/01/rollout-parent.jsonl",&[
        json!({"type":"session_meta","payload":{"id":"thread"}}),ctx("example-model","high","t"),
        event("thread",BEFORE,json!({"type":"WebSearch","action":{"type":"search"}})),
        event("thread",NOW,json!({"type":"UserMessage","content":[{"type":"text","text":"/rename Editor"}]})),
        web.clone(),web,
        event("thread",NOW,json!({"type":"CommandExecution","exit_code":0,"parsed_cmd":[{"cmd":"npx -y ctx7@latest docs /org/package 'question'"}]})),
        event("thread",NOW,json!({"type":"CommandExecution","exit_code":0,"command":["sh","-c","bash /sample/agmsg/scripts/send.sh team author frank hello"],"aggregated_output":"Sent to frank in team sample"})),
        event("thread",NOW,json!({"type":"CommandExecution","exit_code":0,"command":["sh","-c","cat <<EOF\nbash /x/send.sh demo author nope"],"aggregated_output":"Sent to nope in team sample"})),
        event("thread",NOW,json!({"type":"SubAgentActivity","kind":"interacted","agent_path":"reviewer"})),
        event("another",NOW,json!({"type":"SubAgentActivity","kind":"interacted","agent_path":"unrelated"}))]);
    f.log("codex/archived_sessions/rollout-side.jsonl",&[
        json!({"type":"session_meta","payload":{"id":"child","source":{"subagent":{"thread_spawn":{"parent_thread_id":"thread"}}}}}),
        json!({"type":"session_meta","payload":{"id":"thread"}}),ctx("helper-model","low","t"),
        event("child",NOW,json!({"type":"McpToolCall","tool":"query-docs","status":"completed"}))]);
    assert_eq!(
        f.text(&[], &[("CODEX_SESSION_ID", "thread")]),
        "_\nexample-model (high), Editor\nCodex (thread)\nSub-agents: 1 × helper-model (low)\nTeam: frank, reviewer\nResearch: 2 searches, 1 pages, 2 doc lookups\n"
    );
}

#[test]
fn claude_shares_the_commit_cutoff_with_its_subagents() {
    let f = Fixture::new();
    let call = |name: &str, input: Value| json!({"type":"tool_use","name":name,"input":input});
    let calls =
        |date: &str, tools: Vec<Value>| json!({"timestamp":date,"message":{"content":tools}});
    f.log(".claude/projects/project/claude-id.jsonl",&[
        json!({"type":"assistant","requestedModel":"claude-example"}),
        json!({"type":"assistant","requestedModel":"helper-only","isSidechain":true}),
        calls(BEFORE,vec![call("WebFetch",json!({}))]),
        calls(NOW,vec![call("WebSearch",json!({})),call("SendMessage",json!({"to":"maintainer [busy]"})),
            call("Bash",json!({"command":"for peer in one two; do /example/agmsg/scripts/send.sh team actor $peer hi; done"}))])]);
    f.log(".claude/projects/project/claude-id/subagents/agent-a.jsonl",&[
        json!({"type":"assistant","effort":"max","requestedModel":"assistant-model","timestamp":NOW}),
        calls(NOW,vec![call("WebFetch",json!({})),call("mcp__context7__query_docs",json!({}))])]);
    fs::write(
        f.path(".claude/projects/project/claude-id/custom-title.json"),
        r#"{"customTitle":"Writer"}"#,
    )
    .unwrap();
    assert_eq!(
        f.text(
            &[],
            &[
                ("CLAUDE_CODE_SESSION_ID", "claude-id"),
                ("CLAUDE_EFFORT", "max")
            ]
        ),
        "_\nclaude-example (max), Writer\nClaude Code (claude-id)\nSub-agents: 1 × assistant-model (max)\nTeam: maintainer, one, two\nResearch: 1 searches, 1 pages, 1 doc lookups\n"
    );
}

#[test]
fn antigravity_unions_log_and_database_descendants_and_ignores_unrelated_rows() {
    let f = Fixture::new();
    let path = |id: &str, name: &str| format!("agy/brain/{id}/.system_generated/logs/{name}.jsonl");
    let plan = |when: &str, model: &str, tools: Vec<Value>| json!({"type":"PLANNER_RESPONSE","model":model,"created_at":when,"tool_calls":tools});
    f.log(&path("root","transcript_full"),&[
        plan(BEFORE,"old-low",vec![json!({"name":"search_web"})]),
        json!({"type":"GENERIC","content":"Created At: start\nCompleted At: end\nCreated the following subagent:\n{\n  \"conversationId\": \"c1\"\n}"}),
        plan(NOW,"example-high",vec![json!({"name":"send_message","args":{"Recipient":"peer"}}),json!({"name":"send_message","args":{"Recipient":"c1"}}),json!({"name":"read_url_content"})])]);
    f.log(
        &path("c1", "transcript"),
        &[plan(NOW, "helper-low", vec![json!({"name":"search_web"})])],
    );
    f.log(
        &path("c2", "transcript_full"),
        &[plan(
            NOW,
            "helper-low",
            vec![json!({"name":"run_command","args":{"CommandLine":"ctx7 docs /x/y example"}})],
        )],
    );
    f.log(
        &path("inactive", "transcript_full"),
        &[plan(BEFORE, "old-low", vec![json!({"name":"search_web"})])],
    );
    f.log(
        "agy/history.jsonl",
        &[
            json!({"type":"slash_command","conversationId":"root","display":"/rename Author"}),
            json!({"type":"slash_command","conversationId":"peer","display":"/rename Verifier"}),
            json!({"type":"slash_command","conversationId":"other","display":"/rename Unrelated"}),
        ],
    );
    let db = Connection::open(f.path("agy/conversation_summaries.db")).unwrap();
    db.execute_batch("CREATE TABLE conversation_summaries(conversation_id TEXT, parent_conversation_id TEXT);
        INSERT INTO conversation_summaries VALUES('c1','root'),('c2','c1'),('root','c2'),('inactive','root');").unwrap();
    drop(db);
    assert_eq!(
        f.text(&[], &[("ANTIGRAVITY_CONVERSATION_ID", "root")]),
        "_\nexample (high), Author\nAntigravity (root)\nSub-agents: 2 × helper (low)\nTeam: Verifier\nResearch: 1 searches, 1 pages, 1 doc lookups\n"
    );
}

#[test]
fn missing_or_conflicting_evidence_never_selects_an_author_arbitrarily() {
    let f = Fixture::new();
    assert!(!f.run(&[], &[]).status.success());
    let both = [("CODEX_THREAD_ID", "c"), ("CLAUDECODE", "1")];
    let ambiguous = f.run(&[], &both);
    assert!(!ambiguous.status.success());
    assert!(String::from_utf8_lossy(&ambiguous.stderr).contains("--harness"));
    assert_eq!(
        f.text(&["--harness", "claude-code"], &both),
        "_\nunknown (unknown)\nClaude Code\n"
    );
    assert_eq!(
        f.text(
            &["--harness", "antigravity"],
            &[("ANTIGRAVITY_CONVERSATION_ID", "../../foreign")]
        ),
        "_\nunknown (unknown)\nAntigravity (unknown)\n"
    );
    assert_eq!(
        f.text(&["--harness", "codex"], &[]),
        "_\nunknown (unknown)\nCodex\n"
    );
}

#[test]
fn codex_uses_an_unsigned_git_receipt_from_the_same_checkout_as_a_cutoff() {
    let f = Fixture::new();
    let output = f
        .command("git")
        .args(["rev-parse", "--short", "HEAD"])
        .output()
        .unwrap();
    let short = String::from_utf8(output.stdout).unwrap();
    let short = short.trim();
    // This different thread has no matching signed commit. Its Git receipt is
    // finer-grained than the one-second commit clock.
    let thread = "early-thread";
    let path = f.root().to_str().unwrap();
    #[cfg(windows)]
    let path = path.replace('\\', "/");
    let path = path.replace('%', "%25").replace(' ', "%20");
    let uri = format!("file:///{}", path.trim_start_matches('/'));
    f.log("codex/sessions/rollout-early.jsonl", &[
        json!({"type":"session_meta","payload":{"id":thread}}), ctx("example","high","t"),
        event(thread, "2020-01-01T00:00:00.110Z", json!({"type":"WebSearch","id":"past","action":{"type":"search"}})),
        event(thread, "2020-01-01T00:00:00.125Z", json!({"type":"CommandExecution","id":"commit","cwd":uri,
               "command":["git","commit","-m","example"], "aggregated_output":format!("[branch {short}] example")})),
        event(thread, "2020-01-01T00:00:00.140Z", json!({"type":"WebSearch","id":"current","action":{"type":"search"}})),
        // An unrelated clone could contain the same object. Its later receipt
        // provides no evidence that the current thread committed here.
        event(thread, NOW, json!({"type":"CommandExecution","id":"other","cwd":f.path("another-checkout"),
              "command":["git","commit","-m","example"],"aggregated_output":format!("[other {short}] example")})),
    ]);
    assert_eq!(
        f.text(&[], &[("CODEX_THREAD_ID", thread)]),
        "_\nexample (high)\nCodex (early-thread)\nResearch: 1 searches\n"
    );
}
