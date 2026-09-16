<script setup lang="ts">
defineProps<{ unit: number; horizontal?: boolean }>()
</script>
<template><div class="fader" :class="{horizontal}" aria-hidden="true"><i :style="horizontal?{width:`${unit*100}%`}:{height:`${unit*100}%`}"/><b :style="horizontal?{left:`${unit*100}%`}:{bottom:`${unit*100}%`}"/></div></template>
<style scoped>
.fader{height:140px;width:7px;background:color-mix(in srgb,var(--amber) 24%,var(--shade));margin:10px auto;position:relative;border-radius:5px}.fader i{position:absolute;bottom:0;width:100%;background:#eda952;border-radius:5px}.fader b{position:absolute;left:50%;width:28px;height:15px;transform:translate(-50%,50%);border:1px solid #edba77;border-radius:3px;background:color-mix(in srgb,var(--amber) 40%,var(--shade))}.fader.horizontal{width:calc(100% - 20px);height:7px;margin:12px 10px}.fader.horizontal i{height:100%;left:0}.fader.horizontal b{top:50%;width:15px;height:28px;transform:translate(-50%,-50%)}
</style>
