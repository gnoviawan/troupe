// Fails unless every test file in the list has a suite in the junit report,
// so a file bun test skipped or never loaded cannot pass silently.
//   bun scripts/ci/check-junit-files.ts <file-list> <junit.xml>
const [listPath, junitPath] = process.argv.slice(2)
if (!listPath || !junitPath) {
  console.error('usage: bun scripts/ci/check-junit-files.ts <file-list> <junit.xml>')
  process.exit(2)
}

const files = (await Bun.file(listPath).text()).split('\n').filter(Boolean)
const junit = await Bun.file(junitPath).text()
const reported = new Set(
  Array.from(junit.matchAll(/<testsuite [^>]*\bfile="([^"]+)"/g), (m) => m[1])
)
const missing = files.filter((file) => !reported.has(file))

console.log(
  `${files.length - missing.length}/${files.length} listed files have a suite in ${junitPath}`
)
for (const file of missing) console.log(`missing: ${file}`)
if (files.length === 0 || missing.length > 0) process.exit(1)
