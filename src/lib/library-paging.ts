export const CLIP_PAGE_SIZE = 100;

export type ClipPage<T> = { clips: T[]; total: number; hasMore: boolean };
export type LibraryState<T> = ClipPage<T> & { loaded: boolean; loading: boolean; error: boolean };
export type LibraryWindow = { limit?: number | null; oldest?: boolean };

export class ClipPager<T extends { id: string }> {
  private limit: number | null = CLIP_PAGE_SIZE;
  private oldest = false;
  private offset = 0;
  private generation = 0;
  private pending: Promise<boolean> | undefined;

  constructor(
    private state: LibraryState<T>,
    private fetchPage: (offset: number, limit: number | null, oldest: boolean) => Promise<ClipPage<T>>
  ) {}

  refresh(options: LibraryWindow = {}): Promise<boolean> {
    if (options.limit !== undefined) this.limit = options.limit;
    if (options.oldest !== undefined) this.oldest = options.oldest;
    return this.start(0, this.limit, false);
  }

  loadMore(): Promise<boolean> {
    if (this.state.loading || !this.state.hasMore) return Promise.resolve(false);
    return this.start(this.offset, CLIP_PAGE_SIZE, true);
  }

  // Buscar, filtrar y resolver playlists requiere el catálogo entero, solo bajo demanda.
  loadAll(): Promise<boolean> {
    if (this.state.loaded && !this.state.loading && !this.state.hasMore && !this.state.error) {
      this.limit = null;
      return Promise.resolve(true);
    }
    if (this.limit === null && this.state.loading && this.pending) return this.pending;
    return this.refresh({ limit: null });
  }

  private start(offset: number, limit: number | null, append: boolean): Promise<boolean> {
    const task = this.request(offset, limit, append);
    this.pending = task;
    return task;
  }

  private async request(offset: number, limit: number | null, append: boolean): Promise<boolean> {
    const generation = ++this.generation;
    this.state.loading = true;
    this.state.error = false;
    try {
      const page = await this.fetchPage(offset, limit, this.oldest);
      if (generation !== this.generation) return false;
      const seen = new Set(append ? this.state.clips.map((c) => c.id) : []);
      const added = page.clips.filter((c) => {
        if (seen.has(c.id)) return false;
        seen.add(c.id);
        return true;
      });
      this.state.clips = append ? [...this.state.clips, ...added] : added;
      this.state.total = page.total;
      this.state.hasMore = page.hasMore;
      // Avanzar por lo recibido, aunque aparezcan duplicados al cambiar archivos entre páginas.
      this.offset = offset + page.clips.length;
      if (append) this.limit = this.offset;
      return true;
    } catch {
      if (generation === this.generation) this.state.error = true;
      return false;
    } finally {
      if (generation === this.generation) {
        this.state.loaded = true;
        this.state.loading = false;
        this.pending = undefined;
      }
    }
  }
}
