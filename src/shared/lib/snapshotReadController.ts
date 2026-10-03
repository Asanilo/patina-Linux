export const SNAPSHOT_READ_RETRY_DELAYS_MS = [1000, 3000, 10000] as const;

interface SnapshotReadOptions {
    retryDelaysMs?: readonly number[];
    /** Queue a callback and return its cancellation function. */
    scheduleRetry?: (callback: () => void, delayMs: number) => () => void;
}

/** One in-flight read. Ordinary polls coalesce; invalidations discard old results
 * and schedule one fresh read. Optional retries are finite and apply only to
 * snapshot reads, never commands. Disposal cancels timers and publication. */
export class SnapshotReadController<T> {
    private revision = 0;
    private running = false;
    private dirty = false;
    private disposed = false;
    private retryAttempt = 0;
    private retryGeneration = 0;
    private retryScope: string | number | undefined;
    private cancelPendingRetry: (() => void) | undefined;
    private readonly retryDelaysMs: readonly number[];
    private readonly scheduleRetry: NonNullable<SnapshotReadOptions["scheduleRetry"]>;
    private readonly read: () => Promise<T>;
    private readonly publish: (value: T) => void;
    private readonly failed: (error: unknown) => void;
    private readonly readScope: () => string | number;
    constructor(read: () => Promise<T>, publish: (value: T) => void, failed: (error: unknown) => void, readScope: () => string | number, options: SnapshotReadOptions = {}) {
        this.read = read;
        this.publish = publish;
        this.failed = failed;
        this.readScope = readScope;
        this.retryDelaysMs = [...(options.retryDelaysMs ?? [])];
        if (this.retryDelaysMs.some(delay => !Number.isFinite(delay) || delay < 0)) {
            throw new Error("Invalid snapshot retry delay");
        }
        this.scheduleRetry = options.scheduleRetry ?? ((callback, delayMs) => {
            const timer = setTimeout(callback, delayMs);
            return () => clearTimeout(timer);
        });
    }
    refresh(invalidate = false): void {
        if (this.disposed) {
            return;
        }
        if (invalidate) {
            this.revision++;
            this.dirty = true;
            this.retryAttempt = 0;
        }
        this.cancelRetry();
        if (this.running) {
            return;
        }
        this.running = true;
        this.dirty = false;
        const revision = this.revision;
        const scope = this.readScope();
        if (scope !== this.retryScope) {
            this.retryAttempt = 0;
            this.retryScope = scope;
        }
        const current = () => !this.disposed && revision === this.revision && scope === this.readScope();
        void Promise.resolve().then(async () => {
            if (!current()) return;
            try {
                const value = await this.read();
                if (current()) {
                    this.retryAttempt = 0;
                    this.publish(value);
                }
            } catch (error) {
                if (current()) {
                    this.failed(error);
                    const delay = this.retryDelaysMs[this.retryAttempt];
                    if (current() && delay !== undefined) {
                        this.retryAttempt++;
                        const generation = ++this.retryGeneration;
                        this.cancelPendingRetry = this.scheduleRetry(() => {
                            if (this.disposed || generation !== this.retryGeneration) return;
                            this.cancelPendingRetry = undefined;
                            if (scope !== this.readScope()) this.retryAttempt = 0;
                            this.refresh();
                        }, delay);
                    }
                }
            }
        })
            .finally(() => {
            this.running = false;
            if (!this.disposed && (this.dirty || scope !== this.readScope())) {
                if (scope !== this.readScope()) this.retryAttempt = 0;
                this.refresh();
            }
        });
    }
    dispose(): void {
        this.disposed = true;
        this.revision++;
        this.cancelRetry();
    }

    private cancelRetry(): void {
        this.retryGeneration++;
        this.cancelPendingRetry?.();
        this.cancelPendingRetry = undefined;
    }
}
