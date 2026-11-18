import { describe, expect, it } from 'vitest';
import { readFileSync, readdirSync } from 'node:fs';
import { join } from 'node:path';
import { importDocumentText } from '../src/lib/import';
import { detectDocumentKind } from '../src/lib/import';

// WO-03 M1 integration: when the gate sets ARAIGUMA_LIVE_DOCS to a
// directory of real CLI-produced documents (see scripts/live-check.sh and
// scripts/test-all.sh), every one of them must import cleanly through the
// actual frontend validators — the Rust author and the TypeScript reader
// agreeing on the same bytes. Skipped (visibly) when the variable is not
// set, e.g. during isolated frontend development.

const liveDocsDir = process.env.ARAIGUMA_LIVE_DOCS;

describe('live CLI documents import through the frontend validators', () => {
  it.skipIf(!liveDocsDir)('imports every exported live document', () => {
    expect(liveDocsDir).toBeDefined();
    const files = readdirSync(liveDocsDir!).filter((name) => name.endsWith('.json'));
    expect(files.length).toBeGreaterThanOrEqual(2);
    const kinds = new Set<string>();
    for (const name of files) {
      const text = readFileSync(join(liveDocsDir!, name), 'utf8');
      const documents = importDocumentText(text);
      if (documents.snapshot) {
        kinds.add('snapshot');
        expect(documents.snapshot.interfaces.length).toBeGreaterThan(0);
      }
      if (documents.topology) {
        kinds.add('topology');
        expect(documents.topology.nodes.length).toBeGreaterThan(0);
      }
    }
    expect(kinds).toEqual(new Set(['snapshot', 'topology']));
  });

  it('detects the kinds of the live documents when present', () => {
    if (!liveDocsDir) return;
    for (const name of readdirSync(liveDocsDir).filter((n) => n.endsWith('.json'))) {
      // Must not throw: kind detection is part of the import path.
      detectDocumentKind(JSON.parse(readFileSync(join(liveDocsDir, name), 'utf8')));
    }
  });
});
