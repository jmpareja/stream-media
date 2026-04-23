import { useEffect, useState } from 'react'
import { Link, NavLink, Outlet, useLocation, useNavigate } from 'react-router-dom'
import { useAuth } from '../auth'

export default function Layout() {
  const [menuOpen, setMenuOpen] = useState(false)
  const location = useLocation()
  const navigate = useNavigate()
  const { user, logout } = useAuth()

  // Close the drawer whenever the route changes.
  useEffect(() => setMenuOpen(false), [location.pathname])

  // ESC closes the drawer.
  useEffect(() => {
    if (!menuOpen) return
    const onKey = (e: KeyboardEvent) => {
      if (e.key === 'Escape') setMenuOpen(false)
    }
    window.addEventListener('keydown', onKey)
    return () => window.removeEventListener('keydown', onKey)
  }, [menuOpen])

  function handleLogout() {
    logout()
    setMenuOpen(false)
    navigate('/login', { replace: true })
  }

  return (
    <>
      <header className="topbar">
        <button
          type="button"
          className="burger"
          aria-label="Open menu"
          aria-expanded={menuOpen}
          aria-controls="nav-drawer"
          onClick={() => setMenuOpen((v) => !v)}
        >
          <span />
          <span />
          <span />
        </button>
        <Link to="/" className="brand">
          stream-media
        </Link>
      </header>

      {menuOpen && <div className="drawer-backdrop" onClick={() => setMenuOpen(false)} />}

      <nav
        id="nav-drawer"
        className={`drawer ${menuOpen ? 'open' : ''}`}
        aria-hidden={!menuOpen}
      >
        <ul>
          <li>
            <NavLink to="/" end>
              Home
            </NavLink>
          </li>
          {user ? (
            <>
              <li>
                <NavLink to="/profile">Profile</NavLink>
              </li>
              <li>
                <button type="button" className="link-button" onClick={handleLogout}>
                  Sign out
                </button>
              </li>
            </>
          ) : (
            <li>
              <NavLink to="/login">Sign in</NavLink>
            </li>
          )}
        </ul>
        {user && <div className="drawer-user">Signed in as {user.username}</div>}
      </nav>

      <main className="app">
        <Outlet />
      </main>
    </>
  )
}
