import { useTranslation } from 'react-i18next';

import { SegmentedControl } from '../../components/ui/SegmentedControl.tsx';
import type { Language } from '../../lib/settings.ts';
import { useSettings } from '../../stores/settings.ts';
import { Group, Row } from './Row.tsx';

/** Settings → General: what concerns the whole app. */
export function GeneralSettings() {
  const { t } = useTranslation();
  const language = useSettings((s) => s.settings.language);
  const update = useSettings((s) => s.update);

  return (
    <Group>
      <Row label={t('settings.language')}>
        <SegmentedControl<Language>
          label={t('settings.language')}
          value={language}
          options={[
            { value: 'system', label: t('settings.languages.system') },
            { value: 'en', label: t('settings.languages.en') },
            { value: 'ru', label: t('settings.languages.ru') },
          ]}
          onChange={(next) => void update((current) => ({ ...current, language: next }))}
        />
      </Row>
    </Group>
  );
}
