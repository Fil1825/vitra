import { useTranslation } from 'react-i18next';

import { formatTotal } from '../../lib/format.ts';

/** "12 songs, 48 min". */
export function useListSummary() {
  const { t } = useTranslation();
  return (count: number, seconds: number) =>
    [
      t('library.songCount', { count }),
      seconds > 0 &&
        formatTotal(seconds, { hours: t('library.hours'), minutes: t('library.minutes') }),
    ]
      .filter(Boolean)
      .join(', ');
}
