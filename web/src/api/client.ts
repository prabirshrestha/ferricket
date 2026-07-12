export class ApiError<T = unknown> extends Error {
  constructor(
    message: string,
    public status: number,
    public latest?: T,
  ) {
    super(message)
  }
}

export async function request<T>(url: string, init?: RequestInit): Promise<T> {
  const response = await fetch(url, {
    ...init,
    headers: { "Content-Type": "application/json", ...init?.headers },
  })
  if (!response.ok) {
    const result = await response.json().catch(() => ({ error: response.statusText }))
    throw new ApiError(result.error, response.status, result.latest)
  }
  return response.json()
}
