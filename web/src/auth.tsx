import { createContext, useCallback, useContext, useEffect, useState, type ReactNode } from 'react'
import { login as apiLogin, type User } from './api'

const STORAGE_KEY = 'stream-media.user'

type AuthContextValue = {
  user: User | null
  login: (identifier: string, password: string) => Promise<void>
  logout: () => void
}

const AuthContext = createContext<AuthContextValue | null>(null)

function loadUser(): User | null {
  const raw = localStorage.getItem(STORAGE_KEY)
  if (!raw) return null
  try {
    return JSON.parse(raw) as User
  } catch {
    localStorage.removeItem(STORAGE_KEY)
    return null
  }
}

export function AuthProvider({ children }: { children: ReactNode }) {
  const [user, setUser] = useState<User | null>(() => loadUser())

  useEffect(() => {
    if (user) localStorage.setItem(STORAGE_KEY, JSON.stringify(user))
    else localStorage.removeItem(STORAGE_KEY)
  }, [user])

  const login = useCallback(async (identifier: string, password: string) => {
    const u = await apiLogin(identifier, password)
    setUser(u)
  }, [])

  const logout = useCallback(() => setUser(null), [])

  return <AuthContext.Provider value={{ user, login, logout }}>{children}</AuthContext.Provider>
}

export function useAuth(): AuthContextValue {
  const ctx = useContext(AuthContext)
  if (!ctx) throw new Error('useAuth must be used inside AuthProvider')
  return ctx
}
