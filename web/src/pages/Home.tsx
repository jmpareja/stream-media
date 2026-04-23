import { useEffect, useState } from 'react'
import { listMedia, type MediaItem } from '../api'

function formatBytes(bytes: number): string {
  if (bytes < 1024) return `${bytes} B`
  const units = ['KB', 'MB', 'GB', 'TB']
  let n = bytes / 1024
  let i = 0
  while (n >= 1024 && i < units.length - 1) {
    n /= 1024
    i++
  }
  return `${n.toFixed(1)} ${units[i]}`
}

export default function Home() {
  const [items, setItems] = useState<MediaItem[]>([])
  const [total, setTotal] = useState(0)
  const [loading, setLoading] = useState(true)
  const [error, setError] = useState<string | null>(null)

  useEffect(() => {
    listMedia()
      .then((r) => {
        setItems(r.items)
        setTotal(r.total)
      })
      .catch((e: unknown) => setError(e instanceof Error ? e.message : String(e)))
      .finally(() => setLoading(false))
  }, [])

  return (
    <>
      <h2>Media catalog</h2>

      {loading && <p>Loading…</p>}
      {error && <p className="error">Failed to load: {error}</p>}

      {!loading && !error && (
        <>
          <p className="count">
            {total} item{total === 1 ? '' : 's'}
          </p>
          <ul className="media-list">
            {items.map((m) => (
              <li key={m.id} className="media-item">
                <div className="media-title">{m.title}</div>
                <div className="media-meta">
                  <span>{m.media_type}</span>
                  <span>{m.format}</span>
                  <span>{formatBytes(m.file_size)}</span>
                  <span className={`status status-${m.transcode_status}`}>{m.transcode_status}</span>
                </div>
                {m.description && <p className="media-desc">{m.description}</p>}
              </li>
            ))}
          </ul>
        </>
      )}
    </>
  )
}
