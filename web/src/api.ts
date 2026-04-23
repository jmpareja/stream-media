export type User = {
  id: string
  username: string
  email: string
  display_name: string | null
  is_admin: boolean
  created_at: string
  updated_at: string
}

export type MediaItem = {
  id: string
  title: string
  description?: string | null
  media_type: string
  format: string
  file_size: number
  duration_secs?: number | null
  transcode_status: string
  created_at: string
}

export type ListMediaResponse = {
  items: MediaItem[]
  total: number
}

async function parseError(res: Response): Promise<string> {
  try {
    const body = (await res.json()) as { error?: string }
    if (body.error) return body.error
  } catch {
    /* fall through */
  }
  return `HTTP ${res.status}`
}

export async function login(identifier: string, password: string): Promise<User> {
  const res = await fetch('/api/users/login', {
    method: 'POST',
    headers: { 'content-type': 'application/json' },
    body: JSON.stringify({ identifier, password }),
  })
  if (!res.ok) throw new Error(await parseError(res))
  return (await res.json()) as User
}

export async function listMedia(): Promise<ListMediaResponse> {
  const res = await fetch('/api/media')
  if (!res.ok) throw new Error(await parseError(res))
  return (await res.json()) as ListMediaResponse
}
