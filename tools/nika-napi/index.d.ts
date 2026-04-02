/**
 * @supernovae-st/nika-sdk — Node.js SDK for the Nika workflow engine
 *
 * Handwritten type definitions with proper discriminated unions.
 * Supersedes napi-generated types at publish time.
 *
 * @license AGPL-3.0-or-later
 */

// ═══════════════════════════════════════════════════════════════════════════
// DISCRIMINATED EVENT UNION
// ═══════════════════════════════════════════════════════════════════════════

export interface StartedEvent {
  type: 'started';
  jobId: string;
}

export interface TaskStartEvent {
  type: 'task_start';
  jobId: string;
  taskId: string;
  verb: string;
}

export interface TaskCompleteEvent {
  type: 'task_complete';
  jobId: string;
  taskId: string;
  durationMs: number;
}

export interface TaskFailedEvent {
  type: 'task_failed';
  jobId: string;
  taskId: string;
  error: string;
  durationMs: number;
}

export interface ArtifactWrittenEvent {
  type: 'artifact_written';
  jobId: string;
  taskId: string;
  path: string;
  size: number;
}

export interface CompletedEvent {
  type: 'completed';
  jobId: string;
  output?: string;
}

export interface FailedEvent {
  type: 'failed';
  jobId: string;
  error?: string;
}

export interface CancelledEvent {
  type: 'cancelled';
  jobId: string;
}

/**
 * Discriminated union of all event types.
 *
 * Use `event.type` to narrow:
 * ```ts
 * for await (const event of job.events()) {
 *   switch (event.type) {
 *     case 'task_complete':
 *       console.log(event.taskId, event.durationMs); // fully typed
 *       break;
 *     case 'completed':
 *       console.log(event.output);
 *       break;
 *   }
 * }
 * ```
 */
export type NikaEvent =
  | StartedEvent
  | TaskStartEvent
  | TaskCompleteEvent
  | TaskFailedEvent
  | ArtifactWrittenEvent
  | CompletedEvent
  | FailedEvent
  | CancelledEvent;

// ═══════════════════════════════════════════════════════════════════════════
// RUN OPTIONS
// ═══════════════════════════════════════════════════════════════════════════

export interface RunOptions {
  /** Workflow inputs as a JSON object. */
  inputs?: Record<string, unknown>;
  /** Resume from a previous job's checkpoints. */
  resumeFrom?: string;
}

// ═══════════════════════════════════════════════════════════════════════════
// JOB INFO / RESULT
// ═══════════════════════════════════════════════════════════════════════════

export interface JobInfo {
  jobId: string;
  status: string;
  workflow: string;
  createdAt: string;
  startedAt?: string;
  completedAt?: string;
  exitCode?: number;
  output?: string;
}

export interface JobResult {
  jobId: string;
  output?: string;
}

export interface ArtifactInfo {
  name: string;
  size: number;
  format?: string;
  contentType: string;
  checksum?: string;
}

// ═══════════════════════════════════════════════════════════════════════════
// ARTIFACT
// ═══════════════════════════════════════════════════════════════════════════

export declare class Artifact {
  /** Artifact name (filename). */
  get name(): string;
  /** Artifact size in bytes. */
  get size(): number;
  /** Content type (MIME). */
  get contentType(): string;
  /** Checksum (if available). */
  get checksum(): string | undefined;
  /** Download the artifact content. */
  download(): Promise<Buffer>;
}

// ═══════════════════════════════════════════════════════════════════════════
// EVENT STREAM (AsyncGenerator)
// ═══════════════════════════════════════════════════════════════════════════

export declare class EventStream implements AsyncIterableIterator<NikaEvent> {
  next(): Promise<IteratorResult<NikaEvent>>;
  [Symbol.asyncIterator](): AsyncIterableIterator<NikaEvent>;
}

// ═══════════════════════════════════════════════════════════════════════════
// JOB
// ═══════════════════════════════════════════════════════════════════════════

export declare class Job {
  /** The unique job identifier. */
  get jobId(): string;
  /** Get current job status. */
  status(): Promise<JobInfo>;
  /** Cancel the job. */
  cancel(): Promise<JobInfo>;
  /** Wait for the job to complete. */
  wait(): Promise<JobResult>;
  /** List artifacts produced by this job. */
  artifacts(): Promise<Artifact[]>;
  /**
   * Stream events as an async iterator.
   *
   * ```ts
   * for await (const event of job.events()) {
   *   console.log(event.type, event.jobId);
   * }
   * ```
   */
  events(): Promise<EventStream>;
  /** Collect all events into an array (non-streaming). */
  collectEvents(): Promise<NikaEvent[]>;
}

// ═══════════════════════════════════════════════════════════════════════════
// CLIENT
// ═══════════════════════════════════════════════════════════════════════════

export declare class Client {
  /**
   * Create a client connected to a remote `nika serve` instance.
   *
   * ```ts
   * const client = new Client('http://localhost:3000', process.env.NIKA_TOKEN);
   * ```
   */
  constructor(url: string, token?: string);
  /** Submit a workflow for execution. */
  submit(workflow: string, options?: RunOptions): Promise<Job>;
  /** Check if the server is healthy. */
  health(): Promise<boolean>;
  /**
   * One-liner: submit + wait for completion.
   *
   * ```ts
   * const result = await client.run('pipeline.nika.yaml', { inputs: { topic: 'AI' } });
   * console.log(result.output);
   * ```
   */
  run(workflow: string, options?: RunOptions): Promise<JobResult>;
}
