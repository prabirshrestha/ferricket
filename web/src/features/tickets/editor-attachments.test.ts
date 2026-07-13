import { expect, test } from "bun:test"
import {
  attachmentMarkdownForEditor,
  attachmentMarkdownForStorage,
  previewableImageTypes,
} from "./editor-attachments"

test("translates portable attachment links for the editor and back", () => {
  const stored =
    "![diagram](attachments/fer-123/diagram.png)\n\n[spec](attachments/fer-123/spec.pdf)"
  const editor = attachmentMarkdownForEditor(stored)
  expect(editor).toContain("/api/tickets/fer-123/attachments/diagram.png")
  expect(editor).toContain("/api/tickets/fer-123/attachments/spec.pdf")
  expect(attachmentMarkdownForStorage(editor)).toBe(stored)
})

test("previews safe raster images but not executable SVG", () => {
  expect(previewableImageTypes.has("image/png")).toBe(true)
  expect(previewableImageTypes.has("image/svg+xml")).toBe(false)
})
