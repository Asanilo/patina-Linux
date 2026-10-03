/** One in-flight read. Ordinary polls coalesce; invalidations discard old results
 * and schedule one fresh read. Disposing prevents all subsequent publication. */
export class SnapshotReadController<T> {
    private revision = 0;
    private running = false;
    private dirty = false;
    private disposed = false;
    private readonly read: () => Promise<T>;
    private readonly publish: (value: T) => void;
    private readonly failed: (error: unknown) => void;
    private readonly readScope: () => string | number;
    constructor(read: () => Promise<T>, publish: (value: T) => void, failed: (error: unknown) => void, readScope: () => string | number) {
        this.read = read;
        this.publish = publish;
        this.failed = failed;
        this.readScope = readScope;
    }
    refresh(invalidate = false): void {
        if (this.disposed) {
            return;
        }
        if (invalidate) {
            this.revision++;
            this.dirty = true;
        }
        if (this.running) {
            return;
        }
        this.running = true;
        this.dirty = false;
        const revision = this.revision;
        const scope = this.readScope();
        const current = () => !this.disposed && revision === this.revision && scope === this.readScope();
        void Promise.resolve().then(async () => {
            if (!current()) return;
            try {
                const value = await this.read();
                if (current()) this.publish(value);
            } catch (error) {
                if (current()) this.failed(error);
            }
        })
            .finally(() => {
            this.running = false;
            if (!this.disposed && (this.dirty || scope !== this.readScope())) {
                this.refresh();
            }
        });
    }
    dispose(): void {
        this.disposed = true;
        this.revision++;
    }
}
