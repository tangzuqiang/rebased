<script setup lang="ts">
defineProps<{ name: string; size?: number }>();
</script>

<template>
  <svg
    :width="size ?? 14"
    :height="size ?? 14"
    viewBox="0 0 16 16"
    fill="none"
    stroke="currentColor"
    stroke-width="1.4"
    stroke-linecap="round"
    stroke-linejoin="round"
  >
    <path v-if="name === 'chevron'" d="M4 6l4 4 4-4" />
    <path v-if="name === 'search'" d="M11 11l3 3M7.5 12.5a5 5 0 1 1 0-10 5 5 0 0 1 0 10z" />
    <path v-if="name === 'gear'" d="M8 10.2A2.2 2.2 0 1 0 8 5.8a2.2 2.2 0 0 0 0 4.4zM8 1.6l.7 1.7 1.8-.3 1 1.6-1.2 1.3.8 1.6-1.7.7-.3 1.8-1.8-.1-1.1 1.5-1.6-1.1-1.6.7.1-1.8L2 9.6l.8-1.6L1.6 6.7l1-1.6 1.8.3L5.1 3.7 6.2 2.2 8 1.6z" />
    <path v-if="name === 'refresh'" d="M13 8a5 5 0 1 1-1.5-3.5M13 3v3H10" />
    <path v-if="name === 'more'" d="M3 8h.01M8 8h.01M13 8h.01" stroke-width="2.2" />
    <path v-if="name === 'fetch'" d="M8 3v8M5 8l3 3 3-3M3 13h10" />
    <path v-if="name === 'pull'" d="M8 2v8M5 7l3 3 3-3M3 13h10M12 4l2 0" />
    <path v-if="name === 'push'" d="M8 13V5M5 8l3-3 3 3M3 3h10" />
    <path v-if="name === 'close'" d="M4 4l8 8M12 4l-8 8" />
    <path v-if="name === 'plus'" d="M8 3v10M3 8h10" />
    <path v-if="name === 'folder'" d="M2 4h5l1 2h6v7H2z" />
    <path v-if="name === 'file'" d="M5 2h5l4 4v8H5zM10 2v4h4" />
    <path v-if="name === 'branch'" d="M4 3v7a3 3 0 0 0 3 3h2M4 3a1.5 1.5 0 1 0 0-3 1.5 1.5 0 0 0 0 3zM12 13a1.5 1.5 0 1 0 0 3 1.5 1.5 0 0 0 0-3zM12 3a1.5 1.5 0 1 0 0-3 1.5 1.5 0 0 0 0 3zM12 3v7" />
    <circle v-if="name === 'dot'" cx="8" cy="8" r="2.2" fill="currentColor" stroke="none" />
    <path
      v-if="name === 'star'"
      d="M8 1.6l1.7 3.5 3.8.6-2.8 2.7.7 3.8L8 10.4 4.6 12.2l.7-3.8-2.8-2.7 3.8-.6z"
      fill="currentColor"
      stroke="none"
    />
    <circle v-if="name === 'circle'" cx="8" cy="8" r="3.2" />
    <path v-if="name === 'sparkle'" d="M8 1.5l1.1 3.4L12.5 6 9.1 7.1 8 10.5 6.9 7.1 3.5 6l3.4-1.1zM12.5 10l.6 1.6L14.8 12l-1.7.5-.6 1.7-.5-1.7L10.3 12l1.7-.4z" fill="currentColor" stroke="none" />
    <path v-if="name === 'terminal'" d="M2.5 3.5h11v9h-11zM5 6.5l2 1.5-2 1.5M8.5 10h3" />
  </svg>
</template>
