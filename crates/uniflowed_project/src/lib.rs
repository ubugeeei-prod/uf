use std::fs;

use camino::{Utf8Path, Utf8PathBuf};
use thiserror::Error;
use uniflowed_config::UniflowedConfig;
use walkdir::WalkDir;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CreateKind {
    AppReact,
    Lib,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CreateOptions {
    pub name: String,
    pub kind: CreateKind,
    pub force: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CreateReport {
    pub root: Utf8PathBuf,
    pub files: Vec<Utf8PathBuf>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProjectFile {
    pub absolute_path: Utf8PathBuf,
    pub relative_path: String,
    pub source: String,
}

#[derive(Debug, Error)]
pub enum ProjectError {
    #[error("refusing to overwrite {0}; pass --force to replace generated files")]
    Exists(Utf8PathBuf),
    #[error("failed to write {path}: {source}")]
    Write {
        path: Utf8PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("failed to read {path}: {source}")]
    Read {
        path: Utf8PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("failed to walk {path}: {source}")]
    Walk {
        path: Utf8PathBuf,
        #[source]
        source: walkdir::Error,
    },
}

pub fn create_project(
    root: &Utf8Path,
    options: &CreateOptions,
) -> Result<CreateReport, ProjectError> {
    let files = match options.kind {
        CreateKind::AppReact => app_react_files(&options.name),
        CreateKind::Lib => lib_files(&options.name),
    };

    let mut written = Vec::with_capacity(files.len());
    for (path, contents) in files {
        let target = root.join(path);
        write_generated_file(&target, &contents, options.force)?;
        written.push(target);
    }

    Ok(CreateReport {
        root: root.to_path_buf(),
        files: written,
    })
}

pub fn collect_source_files(
    root: &Utf8Path,
    config: &UniflowedConfig,
) -> Result<Vec<ProjectFile>, ProjectError> {
    let mut files = Vec::new();
    for entry in WalkDir::new(root) {
        let entry = entry.map_err(|source| ProjectError::Walk {
            path: root.to_path_buf(),
            source,
        })?;
        let path = Utf8PathBuf::from_path_buf(entry.path().to_path_buf()).map_err(|path| {
            ProjectError::Read {
                path: Utf8PathBuf::from(path.display().to_string()),
                source: std::io::Error::new(std::io::ErrorKind::InvalidData, "path is not UTF-8"),
            }
        })?;

        if !entry.file_type().is_file() || !is_source_file(&path) || is_ignored(root, &path, config)
        {
            continue;
        }

        let source = fs::read_to_string(&path).map_err(|source| ProjectError::Read {
            path: path.clone(),
            source,
        })?;
        let relative_path = path
            .strip_prefix(root)
            .map(|path| path.as_str().to_string())
            .unwrap_or_else(|_| path.as_str().to_string());

        files.push(ProjectFile {
            absolute_path: path,
            relative_path,
            source,
        });
    }
    files.sort_by(|a, b| a.relative_path.cmp(&b.relative_path));
    Ok(files)
}

fn write_generated_file(path: &Utf8Path, contents: &str, force: bool) -> Result<(), ProjectError> {
    if path.exists() && !force {
        return Err(ProjectError::Exists(path.to_path_buf()));
    }

    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|source| ProjectError::Write {
            path: parent.to_path_buf(),
            source,
        })?;
    }

    fs::write(path, contents).map_err(|source| ProjectError::Write {
        path: path.to_path_buf(),
        source,
    })
}

fn is_source_file(path: &Utf8Path) -> bool {
    matches!(
        path.extension(),
        Some("flow" | "js" | "jsx" | "mjs" | "cjs")
    )
}

fn is_ignored(root: &Utf8Path, path: &Utf8Path, config: &UniflowedConfig) -> bool {
    let relative = path.strip_prefix(root).unwrap_or(path).as_str();
    config
        .lint
        .ignore
        .iter()
        .any(|ignored| relative.starts_with(ignored.as_str()))
}

fn app_react_files(name: &str) -> Vec<(&'static str, String)> {
    vec![
        ("package.json", app_package_json(name)),
        ("app.flow", app_entry()),
        ("app/_uf.layout.flow", app_layout()),
        ("app/_uf.middleware.flow", app_middleware()),
        ("app/_uf.page.flow", app_page()),
        ("app/_uf.page.native.flow", app_native_page()),
        ("app/_uf.page.test.flow", app_test()),
        ("app/client/Counter.flow", app_client_counter()),
        ("app/client/useCounter.flow", app_client_hook()),
        ("app/styles/tokens.stylex.flow", stylex_tokens()),
        ("server/actions.flow", app_server_actions()),
    ]
}

fn lib_files(name: &str) -> Vec<(&'static str, String)> {
    vec![
        ("package.json", lib_package_json(name)),
        ("index.flow", lib_index()),
        ("index.test.flow", lib_test()),
    ]
}

fn app_package_json(name: &str) -> String {
    format!(
        r#"{{
  "name": "{name}",
  "private": true,
  "type": "module",
  "scripts": {{
    "dev": "uf dev",
    "build": "uf build",
    "check": "uf check",
    "lint": "uf lint",
    "fmt": "uf fmt",
    "test": "uf test"
  }},
  "dependencies": {{
    "@uniflowed/core": "latest"
  }}
}}
"#
    )
}

fn lib_package_json(name: &str) -> String {
    format!(
        r#"{{
  "name": "{name}",
  "type": "module",
  "scripts": {{
    "build": "uf build",
    "check": "uf check",
    "lint": "uf lint",
    "fmt": "uf fmt",
    "test": "uf test"
  }},
  "exports": {{
    ".": "./index.flow"
  }},
  "dependencies": {{
    "@uniflowed/core": "latest"
  }}
}}
"#
    )
}

fn app_entry() -> String {
    r#"// @flow
import { routerView } from '@uniflowed/router';

export default routerView('./app');
"#
    .to_string()
}

fn app_layout() -> String {
    r#"// @flow
import * as React from '@uniflowed/react';
import { Suspense } from '@uniflowed/react';

component Layout(children: React.Node) {
  return (
    <html lang="en">
      <body>
        <Suspense fallback={null}>{children}</Suspense>
      </body>
    </html>
  );
}

export default Layout;
"#
    .to_string()
}

fn app_middleware() -> String {
    r#"// @flow
import { next } from '@uniflowed/router';

export default function middleware() {
  return next();
}
"#
    .to_string()
}

fn app_page() -> String {
    r#"// @flow
import * as React from '@uniflowed/react';
import { use } from '@uniflowed/react';
import { cell } from '@uniflowed/flow-cell';
import { effect, call } from '@uniflowed/effect';
import { createQuery } from '@uniflowed/query';
import { graphql, useLazyLoadQuery } from '@uniflowed/relay';
import { stylex } from '@uniflowed/stylex';
import { Button, Dialog } from '@uniflowed/ui';
import { refreshGreeting } from '../server/actions.flow';
import Counter from './client/Counter.flow';
import { tokens } from './styles/tokens.stylex.flow';

const selectedTone = cell<'calm' | 'sharp'>('calm');
const HomeQuery = graphql('query HomeQuery { viewer { name } }');

const greetingQuery = createQuery<string>({
  key: ['home', 'greeting'],
  query: () => effect(function* () {
    return yield call(refreshGreeting);
  }),
});

const styles = stylex.create({
  shell: {
    minHeight: '100vh',
    display: 'grid',
    placeItems: 'center',
    backgroundColor: tokens.canvas,
    color: tokens.ink,
  },
});

component Page() {
  const greeting = greetingQuery.use();
  const viewer = use(useLazyLoadQuery<{ +viewer: { +name: string } }>(HomeQuery, {}));

  return (
    <main {...stylex.props(styles.shell)}>
      <h1>{greeting.value ?? viewer.viewer.name}</h1>
      <p>tone: {selectedTone.get()}</p>
      <Counter initial={1} />
      <Dialog.Root>
        <Dialog.Trigger>Open</Dialog.Trigger>
        <Dialog.Body>
          <Button>Native UI, preset styles, RSC split</Button>
        </Dialog.Body>
      </Dialog.Root>
    </main>
  );
}

export default Page;
"#
    .to_string()
}

fn app_server_actions() -> String {
    r#""use server";
// @flow
import { serverAction } from '@uniflowed/server';

export const refreshGreeting = serverAction(async (): Promise<string> => {
  return 'Flow at native speed';
});
"#
    .to_string()
}

fn app_native_page() -> String {
    r#"// @flow
import * as React from '@uniflowed/react';
import { Text, View } from '@uniflowed/react-native';
import { stylex } from '@uniflowed/stylex';

const styles = stylex.create({
  shell: {
    flex: 1,
    alignItems: 'center',
    justifyContent: 'center',
  },
});

component Page() {
  return (
    <View {...stylex.props(styles.shell)}>
      <Text>Flow at native speed</Text>
    </View>
  );
}

export default Page;
"#
    .to_string()
}

fn app_client_counter() -> String {
    r#""use client";
// @flow
import * as React from '@uniflowed/react';
import { Button } from '@uniflowed/ui';
import { useCounter } from './useCounter.flow';

component Counter(initial: number) {
  const [count, increment] = useCounter(initial);

  return <Button onClick={increment}>count: {count}</Button>;
}

export default Counter;
"#
    .to_string()
}

fn app_client_hook() -> String {
    r#""use client";
// @flow
import { useState } from '@uniflowed/react';

export hook useCounter(initial: number): [number, () => void] {
  const [count, setCount] = useState(initial);
  return [count, () => setCount(count + 1)];
}
"#
    .to_string()
}

fn app_test() -> String {
    r#"// @flow
import * as React from '@uniflowed/react';
import { describe, expect, it } from '@uniflowed/testing';
import { render, screen } from '@uniflowed/react-testing';
import Page from './_uf.page.flow';

describe('Page', () => {
  it('renders the starter headline', async () => {
    render(<Page />);
    await expect(screen.findByText('Flow at native speed')).resolves.toBeVisible();
  });
});
"#
    .to_string()
}

fn stylex_tokens() -> String {
    r#"// @flow
import { stylex } from '@uniflowed/stylex';

export const tokens = stylex.defineVars({
  canvas: '#f7f7f2',
  ink: '#151b1f',
});
"#
    .to_string()
}

fn lib_index() -> String {
    r#"// @flow
export opaque type UniflowedId = string;

export function createId(raw: string): UniflowedId {
  return raw;
}
"#
    .to_string()
}

fn lib_test() -> String {
    r#"// @flow
import { describe, expect, it } from '@uniflowed/testing';
import { createId } from './index';

describe('createId', () => {
  it('preserves the source value behind an opaque boundary', () => {
    expect(createId('flow')).toBe('flow');
  });
});
"#
    .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn creates_zero_config_react_flow_app() {
        let dir = tempfile::tempdir().unwrap();
        let root = Utf8PathBuf::from_path_buf(dir.path().to_path_buf()).unwrap();

        let report = create_project(
            &root,
            &CreateOptions {
                name: "hello-uniflowed".to_string(),
                kind: CreateKind::AppReact,
                force: false,
            },
        )
        .unwrap();

        assert_eq!(report.files.len(), 11);
        assert!(root.join("app.flow").exists());
        assert!(root.join("app/_uf.page.flow").exists());
        assert!(root.join("app/_uf.page.native.flow").exists());
        assert!(root.join("server/actions.flow").exists());
        assert!(!root.join("uniflowed.config.flow").exists());

        let page = fs::read_to_string(root.join("app/_uf.page.flow")).unwrap();
        assert!(page.contains("component Page()"));
        assert!(page.contains("@uniflowed/query"));
        assert!(page.contains("@uniflowed/effect"));
        assert!(page.contains("@uniflowed/flow-cell"));
        assert!(page.contains("@uniflowed/stylex"));
        assert!(page.contains("@uniflowed/relay"));
        assert!(page.contains("Dialog.Body"));
        assert!(root.join("app/client/useCounter.flow").exists());
    }

    #[test]
    fn creates_flow_library_template() {
        let dir = tempfile::tempdir().unwrap();
        let root = Utf8PathBuf::from_path_buf(dir.path().to_path_buf()).unwrap();

        create_project(
            &root,
            &CreateOptions {
                name: "flow-lib".to_string(),
                kind: CreateKind::Lib,
                force: false,
            },
        )
        .unwrap();

        let index = fs::read_to_string(root.join("index.flow")).unwrap();
        assert!(index.contains("opaque type UniflowedId"));
    }

    #[test]
    fn refuses_to_overwrite_without_force() {
        let dir = tempfile::tempdir().unwrap();
        let root = Utf8PathBuf::from_path_buf(dir.path().to_path_buf()).unwrap();
        fs::write(root.join("package.json"), "{}").unwrap();

        let error = create_project(
            &root,
            &CreateOptions {
                name: "exists".to_string(),
                kind: CreateKind::Lib,
                force: false,
            },
        )
        .unwrap_err();

        assert!(matches!(error, ProjectError::Exists(_)));
    }

    #[test]
    fn collects_source_files_and_ignores_generated_dirs() {
        let dir = tempfile::tempdir().unwrap();
        let root = Utf8PathBuf::from_path_buf(dir.path().to_path_buf()).unwrap();
        fs::create_dir_all(root.join("app")).unwrap();
        fs::create_dir_all(root.join("dist")).unwrap();
        fs::write(root.join("app/index.flow"), "// @flow\n").unwrap();
        fs::write(root.join("dist/index.js"), "// built\n").unwrap();

        let files = collect_source_files(&root, &UniflowedConfig::default()).unwrap();

        assert_eq!(files.len(), 1);
        assert_eq!(files[0].relative_path, "app/index.flow");
    }
}
