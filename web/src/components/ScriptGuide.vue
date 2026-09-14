<script setup lang="ts">
import { marked } from 'marked'
import guide from '../../../docs/SCRIPTING.md?raw'
// Only this bundled, author-controlled guide is rendered as HTML.
const content=marked.parse(guide,{async:false})
</script>
<template><article class="help-article script-guide" v-html="content" /></template>
<style scoped>
.script-guide{max-width:960px}.script-guide :deep(pre){padding:16px;background:#182123;color:#dce9e8;border:1px solid #435456;border-radius:6px;overflow:auto;font-size:12px;line-height:1.65}.script-guide :deep(table){border-collapse:collapse;width:100%;font-size:13px;display:block;overflow:auto}.script-guide :deep(th),.script-guide :deep(td){padding:9px;border-bottom:1px solid #435456;text-align:left}.script-guide :deep(h2){margin-top:32px}.script-guide :deep(code){overflow-wrap:anywhere}.script-guide :deep(a){color:var(--cyan)}
</style>
