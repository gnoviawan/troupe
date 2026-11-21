// Fails unless the junit report has a suite for exactly the files in the
// list: none missing, none extra. A file bun test skipped or never loaded
// cannot pass silently, and with scripts/ci/shard.ts's coverage check this
// proves the shards together ran every test file once.
//   bun scripts/ci/check-junit-files.ts <file-list> <junit.xml>
const [listPath, junitPath] = process.argv.slice(2)
if (!listPath || !junitPath) {
  console.error('usage: bun scripts/ci/check-junit-files.ts <file-list> <junit.xml>')
  process.exit(2)
}

const files = new Set((await Bun.file(listPath).text()).split('\n').filter(Boolean))
const junit = await Bun.file(junitPath).text()
const reported = new Set(
  Array.from(junit.matchAll(/<testsuite [^>]*\bfile="([^"]+)"/g), (m) => m[1] as string)
)
const missing = [...files].filter((file) => !reported.has(file))
const extra = [...reported].filter((file) => !files.has(file))

console.log(
  `${files.size - missing.length}/${files.size} listed files have a suite in ${junitPath}, ${extra.length} unlisted`
)
for (const file of missing) console.log(`::error::missing from ${junitPath}: ${file}`)
for (const file of extra) console.log(`::error::not in ${listPath}: ${file}`)
if (files.size === 0 || missing.length > 0 || extra.length > 0) process.exit(1)
