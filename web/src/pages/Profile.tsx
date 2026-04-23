import { useNavigate } from 'react-router-dom'
import { useAuth } from '../auth'

export default function Profile() {
  const { user, logout } = useAuth()
  const navigate = useNavigate()

  if (!user) return null

  function handleLogout() {
    logout()
    navigate('/login', { replace: true })
  }

  return (
    <>
      <h2>Profile</h2>
      <dl className="profile">
        <dt>Username</dt>
        <dd>{user.username}</dd>

        <dt>Email</dt>
        <dd>{user.email}</dd>

        <dt>Display name</dt>
        <dd>{user.display_name ?? <em>not set</em>}</dd>

        <dt>Role</dt>
        <dd>{user.is_admin ? 'Admin' : 'User'}</dd>

        <dt>Member since</dt>
        <dd>{new Date(user.created_at).toLocaleDateString()}</dd>
      </dl>
      <button type="button" className="logout" onClick={handleLogout}>
        Sign out
      </button>
    </>
  )
}
