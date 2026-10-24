import { useDraggable } from '@dnd-kit/core';
import {
  AudioWaveform,
  Check,
  Disc3,
  LayoutTemplate,
  ListMusic,
  MicVocal,
  PanelLeft,
  RotateCcw,
  SlidersHorizontal,
  type LucideIcon,
} from 'lucide-react';
import { useEffect, useState } from 'react';
import { useTranslation } from 'react-i18next';

import { Glass } from '../components/Glass/index.ts';
import { showContextMenu, type MenuEntry } from '../lib/menu.ts';
import { useSettings } from '../stores/settings.ts';
import { DEFAULT_LAYOUT, MODULES, isShown, type Layout, type ModuleId } from './model.ts';
import { BUILT_IN_PRESETS } from './presets.ts';
import { setCustomizing, setModuleShown, updateLayout } from './state.ts';
import styles from './Workspace.module.css';

const ICONS: Record<ModuleId, LucideIcon> = {
  sidebar: PanelLeft,
  queue: ListMusic,
  lyrics: MicVocal,
  artwork: Disc3,
  controls: SlidersHorizontal,
  level: AudioWaveform,
};

const report = (what: string) => (err: unknown) => console.error(`${what} failed`, err);

/** A module as a small labelled chip: in the palette, and under the pointer while dragged. */
export function ModuleChip({ module, lifted = false }: { module: ModuleId; lifted?: boolean }) {
  const { t } = useTranslation();
  const Icon = ICONS[module];
  return (
    <span className={styles.chip} data-lifted={lifted || undefined}>
      <Icon size={14} aria-hidden="true" />
      {t(`layout.modules.${module}`)}
    </span>
  );
}

/** A hidden module in the palette: dragged into a zone, or clicked to show where it was. */
function HiddenModule({ module }: { module: ModuleId }) {
  const { t } = useTranslation();
  const { attributes, listeners, setNodeRef } = useDraggable({ id: `palette:${module}` });
  return (
    <button
      ref={setNodeRef}
      type="button"
      className={styles.paletteItem}
      title={t('layout.showHint')}
      onClick={() => setModuleShown(module, true)}
      {...attributes}
      {...listeners}
    >
      <ModuleChip module={module} />
    </button>
  );
}

/**
 * Customize mode's palette, over the bottom of the window: the hidden modules, the presets
 * (Vitra's and the user's), saving the layout as a preset, the default layout, and Done.
 */
export function CustomizePalette({ layout }: { layout: Layout }) {
  const { t } = useTranslation();
  const presets = useSettings((s) => s.settings.layoutPresets);
  const update = useSettings((s) => s.update);
  const lyricsVisible = useSettings((s) => s.settings.lyrics.visible);
  const [naming, setNaming] = useState<string | null>(null);

  useEffect(() => {
    const onKey = (event: KeyboardEvent) => event.key === 'Escape' && setCustomizing(false);
    window.addEventListener('keydown', onKey);
    return () => window.removeEventListener('keydown', onKey);
  }, []);
  // Lyrics hidden everywhere count as hidden here too: showing them brings them back.
  const hidden = MODULES.filter(
    (module) => !isShown(layout, module) || (module === 'lyrics' && !lyricsVisible),
  );

  const apply = (next: Layout) => updateLayout(() => next);
  const savePreset = (name: string) => {
    const trimmed = name.trim();
    if (!trimmed) return;
    void update((s) => ({
      ...s,
      layoutPresets: [
        ...s.layoutPresets.filter((preset) => preset.name !== trimmed),
        { name: trimmed, layout: s.layout },
      ],
    })).catch(report('Saving a layout'));
    setNaming(null);
  };

  const presetMenu = () => {
    const entries: MenuEntry[] = [
      ...BUILT_IN_PRESETS.map((preset) => ({
        label: t(`layout.presets.${preset.id}`),
        action: () => apply(preset.layout),
      })),
    ];
    if (presets.length > 0) {
      entries.push(
        'separator',
        ...presets.map((preset) => ({ label: preset.name, action: () => apply(preset.layout) })),
        {
          label: t('layout.deletePreset'),
          items: presets.map((preset) => ({
            label: preset.name,
            action: () =>
              void update((s) => ({
                ...s,
                layoutPresets: s.layoutPresets.filter((p) => p.name !== preset.name),
              })).catch(report('Deleting a layout')),
          })),
        },
      );
    }
    entries.push('separator', { label: t('layout.savePreset'), action: () => setNaming('') });
    showContextMenu(entries).catch(report('The layout menu'));
  };

  return (
    <div className={styles.paletteWrap}>
      <Glass className={styles.palette} priority={7} aria-label={t('layout.customize')}>
        <div className={styles.paletteRow}>
          <span className={styles.paletteTitle}>{t('layout.customize')}</span>
          <button
            type="button"
            className={`${styles.paletteButton} ${styles.done}`}
            onClick={() => setCustomizing(false)}
          >
            <Check size={14} aria-hidden="true" />
            {t('layout.done')}
          </button>
        </div>
        <div className={styles.paletteItems}>
          {hidden.length > 0 ? (
            hidden.map((module) => <HiddenModule key={module} module={module} />)
          ) : (
            <span className={styles.paletteHint}>{t('layout.allShown')}</span>
          )}
        </div>
        <div className={styles.paletteRow}>
          {naming !== null ? (
            <form
              className={styles.presetForm}
              onSubmit={(event) => {
                event.preventDefault();
                savePreset(naming);
              }}
            >
              <input
                className={styles.presetName}
                // The user asked to name it: the field takes the keyboard at once.
                autoFocus
                maxLength={40}
                placeholder={t('layout.presetName')}
                value={naming}
                onChange={(event) => setNaming(event.currentTarget.value)}
                onKeyDown={(event) => {
                  // Escape leaves the name, not customize mode.
                  if (event.key === 'Escape') {
                    event.stopPropagation();
                    setNaming(null);
                  }
                }}
              />
              <button type="submit" className={styles.paletteButton} disabled={!naming.trim()}>
                {t('layout.save')}
              </button>
            </form>
          ) : (
            <button type="button" className={styles.paletteButton} onClick={presetMenu}>
              <LayoutTemplate size={14} aria-hidden="true" />
              {t('layout.presetsButton')}
            </button>
          )}
          <button
            type="button"
            className={styles.paletteButton}
            onClick={() => apply(DEFAULT_LAYOUT)}
          >
            <RotateCcw size={14} aria-hidden="true" />
            {t('layout.reset')}
          </button>
        </div>
      </Glass>
    </div>
  );
}
