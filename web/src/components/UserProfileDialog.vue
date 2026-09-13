<script setup lang="ts">
import { computed, onBeforeUnmount, onMounted, ref } from 'vue'
import { api } from '../api'
import { avatarUrl, profileFields, profileLabel, type UserProfile } from '../userProfile'
const emit = defineEmits<{ close: []; saved: [user: UserProfile] }>()
const dialog = ref<HTMLDialogElement>(), profile = ref<UserProfile>(), busy = ref(false), error = ref(''), notice = ref('')
const currentPassword = ref(''), password = ref(''), confirmation = ref('')
const avatar = computed(() => profile.value ? avatarUrl(profile.value) : '')
async function save() {
  if (!profile.value) return
  if (password.value !== confirmation.value) { error.value = 'New passwords do not match'; return }
  busy.value = true; error.value = ''; notice.value = ''
  const changedPassword = !!password.value
  try {
    profile.value = await api<UserProfile>('/me', 'PUT', { username: profile.value.username, revision: profile.value.revision,
      fields: Object.fromEntries(profileFields.map(key => [key, profile.value![key] || ''])),
      current_password: currentPassword.value || undefined, password: password.value || undefined })
    currentPassword.value = password.value = confirmation.value = ''
    emit('saved', profile.value)
    notice.value = changedPassword ? 'Profile saved. Other sessions signed out; reconnect audio monitoring if needed.' : 'Profile saved.'
  } catch (reason) { error.value = String(reason) }
  finally { busy.value = false }
}
async function upload(event: Event) {
  const input = event.target as HTMLInputElement, file = input.files?.[0]
  if (!file || !profile.value) return
  busy.value = true; error.value = ''; notice.value = ''
  try {
    const body = new FormData(); body.append('avatar', file)
    const response = await fetch(`/api/users/${profile.value.id}/avatar`, { method: 'PUT', credentials: 'same-origin', headers: { 'X-Pr0former': '1' }, body })
    const result = await response.json()
    if (!response.ok) throw new Error(result.error || 'Avatar upload failed')
    profile.value.avatar_revision = result.revision
    // Refresh the header without treating unsaved profile edits as saved.
    emit('saved', await api<UserProfile>('/me'))
    notice.value = 'Avatar updated.'
  } catch (reason) { error.value = String(reason) }
  finally { busy.value = false; input.value = '' }
}
onMounted(async () => { dialog.value?.showModal(); try { profile.value = await api<UserProfile>('/me') } catch (reason) { error.value = String(reason) } })
onBeforeUnmount(() => dialog.value?.close())
</script>
<template>
<dialog ref="dialog" class="parameter-modal user-profile-dialog" aria-labelledby="user-profile-title" @cancel.prevent="!busy && emit('close')">
  <header class="modal-header"><h2 id="user-profile-title">Edit user profile</h2><button class="icon-button" aria-label="Close user profile" :disabled="busy" @click="emit('close')">×</button></header>
  <form class="profile-content" @submit.prevent="save">
    <p v-if="error" role="alert" class="field-error">{{error}}</p><p v-if="notice" role="status">{{notice}}</p>
    <fieldset v-if="profile" :disabled="busy">
      <section class="profile-avatar"><img v-if="avatar" :src="avatar" alt="Your avatar"><span v-else class="profile-initials">{{profile.username.slice(0,2).toUpperCase()}}</span><label><span class="field-title">Upload avatar<HelpNote label="Avatar image">PNG, JPEG or WebP. Cropped to a 256px square and saved immediately.</HelpNote></span><input aria-label="Upload avatar" type="file" accept="image/png,image/jpeg,image/webp" @change="upload"></label></section>
      <div class="profile-grid"><label>Username<input v-model="profile.username" required minlength="3" maxlength="64" autocomplete="username"></label><label v-for="key in profileFields" :key="key">{{profileLabel(key)}}<textarea v-if="key==='notes'" v-model="profile[key]" maxlength="4096"></textarea><input v-else v-model="profile[key]" :type="key==='email'?'email':'text'" maxlength="256"></label></div>
      <h3 aria-label="Change password">Change password<HelpNote label="Account password policy"><template v-if="profile.is_desktop_session">Your bundled session can set a password without the generated password. It will remain signed in.</template><template v-else>Enter your current password to choose a new one.</template></HelpNote></h3>
      <div class="profile-grid"><label v-if="!profile.is_desktop_session">Current password<input v-model="currentPassword" type="password" autocomplete="current-password" :required="!!password" maxlength="256"></label><label>New password<input v-model="password" type="password" autocomplete="new-password" minlength="8" maxlength="256"></label><label>Confirm new password<input v-model="confirmation" type="password" autocomplete="new-password" :required="!!password" maxlength="256"></label></div>
      <footer class="profile-actions"><button type="submit" class="button primary">{{busy?'Saving…':'Save profile'}}</button><button type="button" class="button" @click="emit('close')">Close</button></footer>
    </fieldset><p v-else-if="!error">Loading profile…</p>
  </form>
</dialog>
</template>
<style scoped>
.user-profile-dialog{width:min(720px,calc(100vw - 24px));max-height:90dvh}.profile-content{padding:24px;overflow:auto}.profile-content fieldset{border:0;padding:0;margin:0;min-width:0}.profile-grid{display:grid;grid-template-columns:repeat(2,minmax(0,1fr));gap:14px}.profile-grid label,.profile-avatar label{display:flex;flex-direction:column;gap:7px;font-size:12px}.profile-grid input,.profile-grid textarea{width:100%;min-width:0;box-sizing:border-box}.profile-content h3{margin-top:24px}.profile-content p,.profile-avatar small{font-size:12px;color:var(--muted);line-height:1.5}.profile-avatar{display:flex;gap:20px;align-items:center;margin-bottom:24px}.profile-avatar img,.profile-initials{width:72px;height:72px;border-radius:50%;object-fit:cover;flex-shrink:0}.profile-initials{display:grid;place-items:center;background:#263b37;color:var(--cyan)}.profile-avatar input{max-width:100%}.profile-actions{display:flex;gap:12px;margin-top:24px}.profile-actions button{min-height:44px}@media(max-width:600px){.profile-grid{grid-template-columns:1fr}.profile-avatar{align-items:flex-start}.profile-avatar label{min-width:0}.profile-content{padding:16px}}
</style>
