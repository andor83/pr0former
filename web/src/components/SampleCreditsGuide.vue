<script setup lang="ts">
import { marked } from 'marked'
import credits from '../../../docs/SAMPLE_CREDITS.md?raw'
// Only this bundled, author-controlled document is rendered as HTML.
const content=marked.parse(credits,{async:false})
</script>
<template><article class="help-article credits-guide" v-html="content" /></template>
<style scoped>
.credits-guide{max-width:960px}.credits-guide :deep(table){border-collapse:collapse;width:100%;font-size:13px;display:block;overflow:auto}.credits-guide :deep(th),.credits-guide :deep(td){border:1px solid #3b4a4c;padding:6px 10px;text-align:left;vertical-align:top}.credits-guide :deep(th){background:#182123}.credits-guide :deep(code){font-size:12px;background:#182123;padding:1px 5px;border-radius:4px}.credits-guide :deep(a){color:var(--cyan);text-decoration:underline}.credits-guide :deep(h2){margin-top:28px}
</style>
