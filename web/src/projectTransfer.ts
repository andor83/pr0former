import type { Project } from './types'

export interface ProjectArchive {
  format: 'pr0former-project'
  version: 1
  project: Project
}

function copy<T>(value: T): T { return JSON.parse(JSON.stringify(value)) }

/**
 * Project files intentionally exclude server- and person-specific bindings.
 * Graph, score, and node configuration remain portable, while the importing
 * workspace assigns its own project ID, revision, membership, and devices.
 */
export function archiveProject(project: Project): ProjectArchive {
  const portable = copy(project)
  portable.id = ''
  portable.revision = 0
  portable.conductor = null
  portable.local_audio_assignments = {}
  portable.conducted = {
    count_in_pulses: portable.conducted?.count_in_pulses ?? 0,
    pulse_unit: portable.conducted?.pulse_unit ?? 4,
    sets: portable.conducted?.sets ?? [],
    midi_bindings: [],
  }
  portable.parts = portable.parts.map(part => ({ ...part, performer: null }))
  return { format: 'pr0former-project', version: 1, project: portable }
}

/** Accept the current archive format and the JSON files exported before it. */
export function readProjectArchive(value: unknown): Project {
  const archive = value as Partial<ProjectArchive>
  const project = archive.format === 'pr0former-project' && archive.version === 1
    ? archive.project
    : value as Project
  if (!project || typeof project !== 'object' || typeof project.name !== 'string' || !project.graph || !Array.isArray(project.parts)) {
    throw new Error('Choose a valid pr0former project JSON file.')
  }
  return archiveProject(project).project
}
