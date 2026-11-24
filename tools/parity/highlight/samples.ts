/**
 * Code the chat shows often, one sample per language: the scripted agent's own fences first,
 * then the common shapes of each language (imports, a type, a function, a call, literals,
 * comments).
 */

export interface Sample {
  lang: string
  code: string
}

export const SAMPLES: Sample[] = [
  {
    lang: 'typescript',
    code: `export function isWorkspaceRoutePath(pathname: string): boolean {
  return pathname === '/' || pathname.startsWith('/c/')
}

const CHAT_ROUTE = /^\\/c\\/[^/]+$/

interface Route {
  path: string
  panes?: boolean
}

// Routes that keep the panes mounted.
export const ROUTES: Route[] = [{ path: '/', panes: true }, { path: '/snapshots', panes: false }]

export async function load(id: number): Promise<string | null> {
  const res = await fetch(\`/api/chat/\${id}\`)
  if (!res.ok) return null
  return res.text()
}`
  },
  {
    lang: 'tsx',
    code: `import { useState } from 'react'

export function Counter({ start = 0 }: { start?: number }): React.JSX.Element {
  const [count, setCount] = useState(start)
  return (
    <button type="button" className="px-2" onClick={() => setCount(count + 1)}>
      Clicked {count} times
    </button>
  )
}`
  },
  {
    lang: 'bash',
    code: `bun vitest run src/renderer/lib
rg -n "CHAT_ROUTE" src/renderer/lib
export TROUPE_SERVER_URL=http://127.0.0.1:8196
for f in *.ts; do echo "$f"; done # list them`
  },
  {
    lang: 'rust',
    code: `use std::collections::HashMap;

/// A session's rows.
#[derive(Debug, Clone)]
pub struct Rows<'a> {
    keys: Vec<&'a str>,
    count: usize,
}

impl<'a> Rows<'a> {
    pub fn new(keys: Vec<&'a str>) -> Self {
        let count = keys.len();
        Self { keys, count }
    }
}

fn main() {
    let mut seen: HashMap<String, u32> = HashMap::new();
    *seen.entry("a".to_string()).or_insert(0) += 1;
    println!("{} rows", seen.len());
}`
  },
  {
    lang: 'python',
    code: `import json
from pathlib import Path

class Store:
    """A tiny JSON store."""

    def __init__(self, root: Path) -> None:
        self.root = root

    def read(self, key: str) -> dict | None:
        path = self.root / f"{key}.json"
        if not path.exists():
            return None  # missing key
        return json.loads(path.read_text())`
  },
  {
    lang: 'json',
    code: `{
  "name": "troupe",
  "version": "0.5.0",
  "private": true,
  "scripts": { "test": "bun run test", "lint": "biome ci ." },
  "workspaces": ["apps/*"],
  "count": 12
}`
  },
  {
    lang: 'css',
    code: `.chat-row:hover {
  background: var(--row-hover);
  padding: 4px 8px;
}

@media (max-width: 640px) {
  #composer { display: none; }
}`
  },
  {
    lang: 'go',
    code: `package main

import "fmt"

type Server struct {
\tPort int
}

func (s *Server) Addr() string {
\treturn fmt.Sprintf("127.0.0.1:%d", s.Port)
}`
  }
]
