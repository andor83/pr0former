export const profileFields = ['prefix', 'first_name', 'middle_name', 'last_name', 'suffix', 'email', 'phone', 'organization', 'notes'] as const
export type UserProfile = { id: string; username: string; is_admin: boolean; revision: number; avatar_revision?: number; is_desktop_session?: boolean } & Partial<Record<typeof profileFields[number], string>>
export const profileLabel = (key: string) => key.replaceAll('_', ' ').replace(/^./, s => s.toUpperCase())
export const avatarUrl = (user: UserProfile) => user.avatar_revision ? `/api/users/${encodeURIComponent(user.id)}/avatar?v=${user.avatar_revision}` : ''
