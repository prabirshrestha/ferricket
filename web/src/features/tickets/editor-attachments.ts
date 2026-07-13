const apiAttachment = /\/api\/tickets\/([^/\s)]+)\/attachments\/([^\s)]+)/g
const storedAttachment = /(?<![/\w])attachments\/([^/\s)]+)\/([^\s)]+)/g

export const maxAttachmentBytes = 20 * 1024 * 1024
export const previewableImageTypes = new Set([
  "image/png",
  "image/jpeg",
  "image/gif",
  "image/webp",
  "image/avif",
])

export function attachmentMarkdownForEditor(markdown: string) {
  return markdown.replace(
    storedAttachment,
    (_match, ticketId: string, fileName: string) =>
      `/api/tickets/${ticketId}/attachments/${fileName}`,
  )
}

export function attachmentMarkdownForStorage(markdown: string) {
  return markdown.replace(
    apiAttachment,
    (_match, ticketId: string, fileName: string) => `attachments/${ticketId}/${fileName}`,
  )
}

export function validateAttachments(files: File[]) {
  const invalid = files.find((file) => file.size === 0 || file.size > maxAttachmentBytes)
  if (!invalid) return null
  return `${invalid.name} must be between 1 byte and 20 MiB.`
}
