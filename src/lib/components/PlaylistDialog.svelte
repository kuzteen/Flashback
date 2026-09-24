<script lang="ts">
  import { untrack } from 'svelte';
  import Icon from './Icon.svelte';
  import CoverFormDialog, { type CoverChange } from './CoverFormDialog.svelte';
  import {
    playlistEdit,
    closePlaylistEdit,
    findPlaylist,
    createPlaylist,
    updatePlaylist,
    setPlaylistCover,
    clearPlaylistCover
  } from '$lib/playlists.svelte';
  import { t } from '$lib/i18n.svelte';

  const creating = $derived(playlistEdit.creating);
  const playlist = $derived(playlistEdit.id ? findPlaylist(playlistEdit.id) : undefined);
  const open = $derived(creating || !!playlist);

  let name = $state('');
  let description = $state('');
  let dialog = $state<ReturnType<typeof CoverFormDialog> | null>(null);

  const MAX_DESC = 100;

  // Los campos se siembran al abrir y al cerrar, no en cada render: leer la playlist sin
  // untrack volvería a sembrarlos con cada cambio de la lista (subir portada, por ejemplo) y
  // pisaría lo que el usuario está escribiendo.
  $effect(() => {
    const id = playlistEdit.id;
    void playlistEdit.creating;
    untrack(() => {
      const p = id ? findPlaylist(id) : undefined;
      name = p?.name ?? '';
      // maxlength no recorta lo que llega por binding: sin esto una descripción guardada con
      // el límite anterior abriría el diálogo con el contador por encima del máximo.
      description = (p?.description ?? '').slice(0, MAX_DESC);
    });
  });

  // El foco va por acción y no dentro de un efecto: leer ahí la referencia del input lo haría
  // depender de ella, y al desmontarse el formulario (paso de recorte) el efecto se
  // reejecutaría pisando lo escrito.
  function focusSelect(node: HTMLInputElement) {
    node.focus();
    node.select();
  }

  async function save(cover: CoverChange) {
    if (playlistEdit.id) {
      await updatePlaylist(playlistEdit.id, name, description);
      if (cover.bytes) await setPlaylistCover(playlistEdit.id, cover.bytes);
      else if (cover.clear) await clearPlaylistCover(playlistEdit.id);
    } else {
      await createPlaylist(name, description, cover.bytes);
    }
    closePlaylistEdit();
  }
</script>

<CoverFormDialog
  bind:this={dialog}
  {open}
  session={playlistEdit.id ?? (creating ? 'new' : '')}
  title={t(creating ? 'pl.new' : 'pl.edit')}
  coverSrc={playlist?.coverSrc ?? null}
  saveLabel={t(creating ? 'pl.create' : 'pl.save')}
  canSave={!!name.trim()}
  onsave={save}
  oncancel={closePlaylistEdit}
>
  {#snippet placeholder()}
    <Icon name="heart-fill" size={48} />
  {/snippet}
  {#snippet fields()}
    <label class="field name">
      <span class="notch">{t('pl.fieldName')}</span>
      <input
        use:focusSelect
        bind:value={name}
        maxlength="60"
        placeholder={t('pl.namePlaceholder')}
        onkeydown={(e) => e.key === 'Enter' && dialog?.save()}
      />
    </label>
    <label class="field desc">
      <span class="notch">{t('pl.fieldDesc')}</span>
      <textarea bind:value={description} maxlength={MAX_DESC} placeholder={t('pl.descPlaceholder')}></textarea>
      <span class="count mono">{description.length}/{MAX_DESC}</span>
    </label>
  {/snippet}
</CoverFormDialog>

<style>
  /* Dentro del recuadro: el textarea reserva sitio abajo y el contador lleva el fondo del
     campo para que el texto al desbordar no se lea por debajo. */
  .count {
    position: absolute;
    right: 9px;
    bottom: 7px;
    padding-left: 6px;
    font-size: 10.5px;
    color: var(--text-3);
    background: var(--bg-0);
    pointer-events: none;
  }
</style>
