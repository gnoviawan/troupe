// The frontend test files: tracked files that match these globs. bun test
// runs exactly this list (`--files-from`). Run directly to print it.
//   bun scripts/ci/test-files.ts
const TEST_FILE_GLOBS = [
  'src/**/*.test.{ts,tsx}',
  'scripts/**/*.test.ts',
  'design-system/**/*.test.{ts,tsx}',
  // Pure e2e-harness helpers; Playwright only collects `*.spec.ts`.
  'tests/e2e/**/*.test.ts'
].map((pattern) => new Bun.Glob(pattern))

export async function listTestFiles(): Promise<string[]> {
  const tracked = (await Bun.$`git ls-files -z`.text()).split('\0')
  return tracked.filter((file) => TEST_FILE_GLOBS.some((glob) => glob.match(file))).sort()
}

if (import.meta.main) {
  for (const file of await listTestFiles()) console.log(file)
}
