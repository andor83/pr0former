import type HelpNote from './components/HelpNote.vue'
declare module 'vue' {
  interface GlobalComponents { HelpNote: typeof HelpNote }
}
export {}
