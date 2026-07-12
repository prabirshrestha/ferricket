import { readFile, rm, writeFile } from "node:fs/promises"
import path from "node:path"
import tailwind from "bun-plugin-tailwind"

const outdir = path.join(process.cwd(), "dist")
await rm(outdir, { recursive: true, force: true })

const workerResult = await Bun.build({
  entrypoints: ["src/features/tickets/dependency-layout-worker.ts"],
  outdir,
  naming: "dag-layout-[hash].[ext]",
  minify: true,
  target: "browser",
  format: "esm",
})

if (!workerResult.success) {
  for (const log of workerResult.logs) console.error(log)
  process.exit(1)
}

const workerOutput = workerResult.outputs.find((output) => output.kind === "entry-point")
if (!workerOutput) throw new Error("DAG layout worker did not produce an entry point")
const workerUrl = "/" + path.basename(workerOutput.path)

const result = await Bun.build({
  entrypoints: ["src/index.html"],
  outdir,
  plugins: [tailwind],
  minify: true,
  target: "browser",
  define: {
    "process.env.NODE_ENV": JSON.stringify("production"),
    __FERRICKET_DAG_WORKER_URL__: JSON.stringify(workerUrl),
  },
})

if (!result.success) {
  for (const log of result.logs) console.error(log)
  process.exit(1)
}

// The SPA is served at bookmarkable nested routes such as /active/ticket/fer-123.
// Bun emits relative asset URLs for HTML entrypoints, so make them root-relative.
const indexPath = path.join(outdir, "index.html")
const indexHtml = await readFile(indexPath, "utf8")
await writeFile(
  indexPath,
  indexHtml.replaceAll('href="./', 'href="/').replaceAll('src="./', 'src="/'),
)

for (const output of [...workerResult.outputs, ...result.outputs]) {
  console.log(
    " " +
      path.relative(process.cwd(), output.path) +
      "  " +
      (output.size / 1024).toFixed(1) +
      " KB",
  )
}
