<script setup lang="ts">
import { iconPaths } from './icon-data'
import type { IconName } from './icon-data'

const props = withDefaults(defineProps<{ name: IconName; size?: number; stroke?: number }>(), {
  size: 18,
  stroke: 1.75
})
</script>

<template>
  <svg
    :width="props.size"
    :height="props.size"
    viewBox="0 0 24 24"
    fill="none"
    stroke="currentColor"
    :stroke-width="props.stroke"
    stroke-linecap="round"
    stroke-linejoin="round"
    aria-hidden="true"
    focusable="false"
    v-html="iconPaths[props.name]"
  />
</template>
