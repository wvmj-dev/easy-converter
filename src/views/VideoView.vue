<script setup lang="ts">
import { computed, ref } from 'vue'
import { useI18n } from 'vue-i18n'
import DropZone from '../components/DropZone.vue'
import FileList from '../components/FileList.vue'
import OperationPicker, { type OperationChoice } from '../components/OperationPicker.vue'
import MediaQualityOptions from '../components/options/MediaQualityOptions.vue'
import VideoAudioTrackOptions from '../components/options/VideoAudioTrackOptions.vue'
import VideoEmbedSubtitleOptions from '../components/options/VideoEmbedSubtitleOptions.vue'
import VideoConvertOptions from '../components/options/VideoConvertOptions.vue'
import VideoExtractAudioOptions from '../components/options/VideoExtractAudioOptions.vue'
import VideoResizeOptions from '../components/options/VideoResizeOptions.vue'
import VideoToGifOptions from '../components/options/VideoToGifOptions.vue'
import VideoTrimOptions from '../components/options/VideoTrimOptions.vue'
import { useOperationScreen } from '../composables/useOperationScreen'
import type { Problem } from '../operations/image'
import type { ToolId } from '../types/ToolId'
import {
  VIDEO_OPERATIONS,
  buildVideoRequests,
  defaultVideoOptions,
  unavailableReason,
  validateOptions,
  type VideoOperation,
} from '../operations/video'

const { t } = useI18n()
const operation = ref<VideoOperation>('convert')
const options = ref(defaultVideoOptions())
const screen = useOperationScreen('video', 'video', [operation, options])
const { files, filesStore, toolsStore } = screen

const hasTool = (id: ToolId) => toolsStore.isAvailable(id)
const encoders = computed(() => toolsStore.byId('ffmpeg')?.encoders ?? null)
const translate = (problem: Problem | null) => (problem ? t(problem.key, problem.params ?? {}) : null)

const choices = computed<OperationChoice[]>(() =>
  VIDEO_OPERATIONS.map((id) => ({
    id,
    label: t(`video.ops.${id}.label`),
    description: t(`video.ops.${id}.description`),
    unavailable: files.value.length > 0 ? translate(unavailableReason(id, files.value, hasTool)) : null,
  })),
)
const problem = computed(
  () =>
    translate(unavailableReason(operation.value, files.value, hasTool)) ??
    translate(validateOptions(operation.value, options.value, files.value, encoders.value)),
)
const jobCount = computed(() => (operation.value === 'merge' ? 1 : files.value.length))

function start() {
  if (problem.value) return
  void screen.start(buildVideoRequests(operation.value, files.value, options.value))
}
</script>

<template>
  <div class="page">
    <header class="page__header">
      <h1 class="page__title">
        {{ t('video.title') }}
      </h1>
      <p class="page__subtitle">
        {{ t('video.subtitle') }}
      </p>
    </header>
    <DropZone @files="screen.addPaths" />
    <ul
      v-if="screen.addErrors.value.length > 0"
      class="alert alert--danger"
      role="alert"
    >
      <li
        v-for="error in screen.addErrors.value"
        :key="error"
      >
        {{ error }}
      </li>
    </ul>
    <p
      v-if="screen.addNotice.value"
      class="alert alert--info"
      role="status"
    >
      {{ screen.addNotice.value }}
    </p>
    <FileList
      v-if="files.length > 0"
      :files="files"
      :reorderable="operation === 'merge'"
      @remove="filesStore.remove"
      @clear="filesStore.clear('video')"
      @move="filesStore.move"
    />
    <p
      v-if="screen.otherCount.value > 0"
      class="page__subtitle"
    >
      {{ t('video.otherFiles', screen.otherCount.value) }}
    </p>
    <template v-if="files.length > 0">
      <OperationPicker
        v-model="operation"
        name="video-operation"
        :operations="choices"
      />
      <section
        class="card options"
        :aria-label="t('operations.options')"
      >
        <h2 class="section-title">
          {{ t(`video.ops.${operation}.label`) }}
        </h2>
        <VideoConvertOptions
          v-if="operation === 'convert'"
          v-model="options.convert"
        />
        <MediaQualityOptions
          v-else-if="operation === 'compress'"
          v-model="options.compress.quality"
        />
        <VideoResizeOptions
          v-else-if="operation === 'resize'"
          v-model="options.resize"
        />
        <VideoTrimOptions
          v-else-if="operation === 'trim'"
          v-model="options.trim"
        />
        <VideoExtractAudioOptions
          v-else-if="operation === 'extractAudio'"
          v-model="options.extractAudio"
        />
        <VideoToGifOptions
          v-else-if="operation === 'toGif'"
          v-model="options.toGif"
        />
        <VideoAudioTrackOptions
          v-else-if="operation === 'audioTrack'"
          v-model="options.audioTrack"
        />
        <VideoEmbedSubtitleOptions
          v-else-if="operation === 'embedSubtitle'"
          v-model="options.embedSubtitle"
        />
        <p
          v-else
          class="field__help"
        >
          {{ t(`video.ops.${operation}.help`) }}
        </p>
      </section>
      <div class="actions">
        <button
          type="button"
          class="btn btn--primary"
          :disabled="!!problem || screen.starting.value"
          @click="start"
        >
          {{ t('video.start', jobCount) }}
        </button>
        <span
          v-if="problem"
          class="actions__problem"
          role="status"
        >{{ problem }}</span>
      </div>
      <p
        v-if="screen.started.value"
        class="alert alert--success"
        role="status"
      >
        {{ t('video.started', screen.started.value) }}
      </p>
      <p
        v-if="screen.startError.value"
        class="alert alert--danger"
        role="alert"
      >
        {{ screen.startError.value }}
      </p>
    </template>
  </div>
</template>

<style scoped>
.actions {
  display: flex;
  align-items: center;
  flex-wrap: wrap;
  gap: var(--space-3);
}

.actions__problem {
  color: var(--color-warning);
  font-size: 0.875rem;
}
</style>
