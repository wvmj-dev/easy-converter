<script setup lang="ts" generic="T extends string">
import { computed } from 'vue'
import { useI18n } from 'vue-i18n'
import ChoiceField from './ChoiceField.vue'

/**
 * Output format picker. `prefix` is the i18n section with `format.legend`,
 * `format.names.<value>` and, optionally, `format.hints.<value>`.
 */
const props = defineProps<{ name: string; prefix: string; formats: T[] }>()
const format = defineModel<T>({ required: true })
const { t, te } = useI18n()
const choices = computed(() =>
  props.formats.map((value) => ({ value, label: t(`${props.prefix}.format.names.${value}`) })),
)
const hint = computed(() => {
  const key = `${props.prefix}.format.hints.${format.value}`
  return te(key) ? t(key) : null
})
</script>

<template>
  <div class="options">
    <ChoiceField
      v-model="format"
      :legend="t(`${prefix}.format.legend`)"
      :name="name"
      :choices="choices"
    />
    <p
      v-if="hint"
      class="field__help"
    >
      {{ hint }}
    </p>
  </div>
</template>
