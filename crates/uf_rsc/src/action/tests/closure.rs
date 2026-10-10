//! The rewrite that hoists an inline `"use server"` closure.

use std::fs;

use camino::Utf8PathBuf;
use compact_str::CompactString;

use super::super::closure::{ClosureId, rewrite_module};
use super::build_id;
use crate::ServerActionRegistry;
use crate::graph::RscGraphBuilder;
use crate::project::{ProjectScanOptions, analyze_project};
use crate::write_inline_closures;

const PAGE: &str = "\
import db from \"./db.js\";
export default function Page(post) {
  const postId = post.id;
  const save = async (text) => {
    \"use server\";
    await db.write(postId, text);
  };
  return save;
}
";

fn closure(index: usize) -> ClosureId {
    ClosureId {
        index,
        id: CompactString::from("ab".repeat(32)),
    }
}

fn project(files: &[(&str, &str)]) -> (tempfile::TempDir, Utf8PathBuf) {
    let dir = tempfile::tempdir().expect("tempdir");
    let root = Utf8PathBuf::from_path_buf(dir.path().to_path_buf()).expect("utf8 root");
    for (path, contents) in files {
        let target = root.join(path);
        fs::create_dir_all(target.parent().expect("parent")).expect("dirs");
        fs::write(&target, contents).expect("write");
    }
    (dir, root)
}

#[test]
fn an_inline_closure_seals_only_the_bindings_it_uses() {
    let rewritten = rewrite_module(PAGE, &[closure(0)]).expect("rewrite");
    let id = "ab".repeat(32);
    assert!(rewritten.contains(
        "import { bindServerClosure, unsealServerCaptures } from \"@uniflowed/router/rsc\";"
    ));
    assert!(rewritten.contains(&format!(
        "bindServerClosure($$ufAction0, \"{id}\", [postId])"
    )));
    assert!(rewritten.contains("export async function $$ufAction0($$ufSealed, text)"));
    assert!(rewritten.contains(&format!(
        "const [postId] = unsealServerCaptures($$ufSealed, \"{id}\")"
    )));
    assert!(rewritten.contains("await db.write(postId, text)"));
    assert!(!rewritten.contains("\"use server\""));
    assert!(!rewritten.contains("[db"));
    assert!(!rewritten.contains("[post]"));
    assert!(!rewritten.contains("[text]"));
    assert!(!rewritten.contains("[postId,"));
}

#[test]
fn a_sibling_parameter_default_is_not_sealed() {
    let source = "\
export default function Page(post) {
  const save = async (text, other = text, third) => {
    \"use server\";
    return third;
  };
  return save;
}
";
    let rewritten = rewrite_module(source, &[closure(0)]).expect("rewrite");
    assert!(rewritten.contains("bindServerClosure($$ufAction0, \""));
    assert!(rewritten.contains("\", [])"));
    assert!(rewritten.contains("$$ufSealed, text, other = text, third"));
    assert!(rewritten.contains("unsealServerCaptures($$ufSealed,"));
    assert!(!rewritten.contains("const ["));
}

#[test]
fn an_outer_parameter_default_is_sealed() {
    let source = "\
export default function Page(text) {
  const save = async (other = text) => {
    \"use server\";
    return other;
  };
  return save;
}
";
    let rewritten = rewrite_module(source, &[closure(0)]).expect("rewrite");
    assert!(rewritten.contains(", [text])"));
    assert!(rewritten.contains("const [text] = unsealServerCaptures"));
    assert!(rewritten.contains("$$ufSealed, other)"));
    assert!(!rewritten.contains("other = text)"));
    let unseal = rewritten.find("unsealServerCaptures").expect("unseal");
    let defaulted = rewritten
        .find("if (other === undefined) other = text;")
        .expect("default");
    assert!(unseal < defaulted);
}

#[test]
fn a_ternary_a_cast_and_a_case_seal_the_read_and_an_object_key_does_not() {
    let source = "\
export default function Page(post) {
  const postId = post.id;
  const save = async (mode) => {
    \"use server\";
    const keyed = { postId: 1 };
    switch (mode) {
      case postId:
        return (postId: number);
    }
    return mode ? postId : keyed;
  };
  return save;
}
";
    let rewritten = rewrite_module(source, &[closure(0)]).expect("rewrite");
    assert!(rewritten.contains(", [postId])"));
    assert!(!rewritten.contains("[postId,"));
}

#[test]
fn an_object_method_keeps_its_key() {
    let source = "\
export default function Page(post) {
  const postId = post.id;
  const api = {
    async save(text) {
      \"use server\";
      return postId + text;
    },
  };
  return api.save;
}
";
    let rewritten = rewrite_module(source, &[closure(0)]).expect("rewrite");
    assert!(rewritten.contains("save: bindServerClosure($$ufAction0,"));
    assert!(rewritten.contains(", [postId])"));
    assert!(!rewritten.contains("const save"));
}

#[test]
fn a_class_method_is_refused() {
    let source = "\
class Store {
  async save() {
    \"use server\";
    return 1;
  }
}
";
    assert!(rewrite_module(source, &[closure(0)]).is_err());
}

#[test]
fn a_pattern_default_that_reads_a_capture_is_refused() {
    let source = "\
export default function Page(post) {
  const save = async ({ id } = post) => {
    \"use server\";
    return id;
  };
  return save;
}
";
    assert!(rewrite_module(source, &[closure(0)]).is_err());
}

#[test]
fn a_nested_closure_is_spliced_into_the_outer_body() {
    let source = "\
export default function Page(post) {
  const postId = post.id;
  const save = async (text) => {
    \"use server\";
    const inner = async (more) => {
      \"use server\";
      return postId + text + more;
    };
    return inner;
  };
  return save;
}
";
    let mut inner = closure(1);
    inner.id = CompactString::from("cd".repeat(32));
    let rewritten = rewrite_module(source, &[closure(0), inner]).expect("rewrite");
    assert!(rewritten.contains("bindServerClosure($$ufAction0, \""));
    assert!(rewritten.contains(", [postId])"));
    assert!(rewritten.contains("bindServerClosure($$ufAction1, \""));
    assert!(rewritten.contains("[postId, text]"));
    assert!(rewritten.contains("export async function $$ufAction0("));
    assert!(rewritten.contains("export async function $$ufAction1($$ufSealed, more)"));
    assert_eq!(rewritten.matches("\"use server\"").count(), 0);
}

#[test]
fn an_exported_function_declaration_keeps_its_name() {
    let source = "\
\"use server\";
export async function save(text) {
  \"use server\";
  return text;
}
";
    let rewritten = rewrite_module(source, &[closure(0)]).expect("rewrite");
    assert!(rewritten.starts_with("\"use server\";"));
    assert!(rewritten.contains("export const save = bindServerClosure($$ufAction0,"));
    assert!(rewritten.contains(", [])"));
    assert!(rewritten.contains("unsealServerCaptures($$ufSealed,"));
    let directive = rewritten.find("\"use server\"").expect("file directive");
    let import = rewritten.find("bindServerClosure").expect("import");
    assert!(directive < import);
}

#[test]
fn a_module_block_binding_is_sealed() {
    let source = "\
{
  const postId = 1;
  const save = async () => {
    \"use server\";
    return postId;
  };
}
";
    let rewritten = rewrite_module(source, &[closure(0)]).expect("rewrite");
    assert!(rewritten.contains(", [postId])"));
}

#[test]
fn an_uppercase_action_id_is_refused() {
    let mut bad = closure(0);
    bad.id = CompactString::from("AB".repeat(32));
    assert!(rewrite_module(PAGE, &[bad]).is_err());
}

#[test]
fn a_flow_comment_stays_before_the_inserted_import() {
    let source = "\
// @flow
import db from \"./db.js\";
export default function Page(post) {
  const postId = post.id;
  const save = async (text) => {
    \"use server\";
    return postId;
  };
  return save;
}
";
    let rewritten = rewrite_module(source, &[closure(0)]).expect("rewrite");
    assert!(rewritten.starts_with("// @flow\nimport { bindServerClosure"));
}

#[test]
fn the_sidecar_rewrites_a_callable_server_closure_and_skips_a_client_one() {
    let mut page = String::from("import Form from \"./Form.js\";\n");
    page.push_str(PAGE);
    let (_dir, root) = project(&[
        ("app/$page.js", &page),
        (
            "app/Form.js",
            "\"use client\";\nexport default function Form() {\n  const save = async () => {\n    \"use server\";\n  };\n  return save;\n}\n",
        ),
    ]);
    let analysis =
        analyze_project(&root, &build_id(), &ProjectScanOptions::default()).expect("analysis");
    let callable = analysis
        .registry
        .callable_actions()
        .filter(|action| action.module.as_str().ends_with("$page.js"))
        .count();
    assert_eq!(callable, 1);
    assert!(analysis.registry.actions().iter().any(|action| {
        action.module.as_str().ends_with("Form.js") && !action.exposure.is_callable()
    }));

    let out = tempfile::tempdir().expect("out");
    let out_root = Utf8PathBuf::from_path_buf(out.path().to_path_buf()).expect("utf8");
    let path = write_inline_closures(&out_root, &root, &analysis.registry).expect("sidecar");
    let json = fs::read_to_string(path).expect("read");
    assert!(json.contains("\"app/$page.js\""));
    assert!(json.contains("bindServerClosure($$ufAction0"));
    assert!(json.contains("[postId]"));
    assert!(!json.contains("\"app/Form.js\""));
    assert!(!json.contains("build-id-for-tests"));
    assert!(!json.contains("use server"));
}

#[test]
fn an_empty_registry_still_writes_the_sidecar() {
    let registry = ServerActionRegistry::from_graph(&RscGraphBuilder::new().build(), &build_id());
    let out = tempfile::tempdir().expect("out");
    let out_root = Utf8PathBuf::from_path_buf(out.path().to_path_buf()).expect("utf8");
    let path = write_inline_closures(&out_root, &out_root, &registry).expect("sidecar");
    let json = fs::read_to_string(path).expect("read");
    assert!(json.contains("\"version\": 1"));
    assert!(json.contains("\"modules\": {}"));
}
