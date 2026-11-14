/**
 * One panel section in its family's shape. `compact` is the Home card: charts
 * shrink to a strip, tables to two columns and lists to their first items,
 * each with how much more the panel page shows.
 */
import { comparisonView, formatValue, progressView, type Section } from '@cuckoo/shared';
import { useState } from 'react';
import { StyleSheet, Text, View } from 'react-native';
import { shortTime } from '../chats/time';
import { type as typography } from '../theme/tokens';
import { useTokens } from '../theme/use-tokens';
import { ChartStrip } from './chart-strip';

const LIMIT = 3;

function More({ count }: { count: number }) {
  const t = useTokens();
  if (count <= 0) return null;
  return <Text style={[typography.small, { color: t.muted }]}>{count} more</Text>;
}

export function SectionView({ section, compact }: { section: Section; compact?: boolean }) {
  const t = useTokens();
  const [width, setWidth] = useState(0);
  const limit = compact ? LIMIT : Infinity;
  const tone = (sentiment: string) => (sentiment === 'favorable' ? t.success : sentiment === 'unfavorable' ? t.danger : t.muted);

  switch (section.kind) {
    case 'text':
      return (
        <View style={styles.gap}>
          {section.heading ? <Text style={[typography.header, { color: t.text }]}>{section.heading}</Text> : null}
          <Text numberOfLines={compact ? 4 : undefined} style={[typography.body, { color: t.text }]}>{section.body}</Text>
        </View>
      );
    case 'metric': {
      const comparison = section.comparison && comparisonView(section.comparison);
      return (
        <View style={styles.gap}>
          <Text style={[typography.meta, { color: t.muted }]}>{section.label}</Text>
          <Text style={[styles.big, { color: t.text }]}>{section.value}</Text>
          {comparison ? <Text style={[typography.meta, { color: tone(comparison.sentiment) }]}>{comparison.text}</Text> : null}
        </View>
      );
    }
    case 'metrics':
      return (
        <View style={styles.grid}>
          {section.items.slice(0, compact ? 4 : undefined).map(item => {
            const comparison = item.comparison && comparisonView(item.comparison);
            return (
              <View key={item.id} style={styles.cell}>
                <Text numberOfLines={1} style={[typography.small, { color: t.muted }]}>{item.label}</Text>
                <Text style={[typography.header, { color: t.text }]}>{item.value}</Text>
                {comparison ? <Text style={[typography.small, { color: tone(comparison.sentiment) }]}>{comparison.text}</Text> : null}
              </View>
            );
          })}
        </View>
      );
    case 'chart':
      return (
        <View style={styles.gap} onLayout={event => setWidth(event.nativeEvent.layout.width)}>
          {section.title ? <Text style={[typography.meta, { color: t.muted }]}>{section.title}</Text> : null}
          <View accessible accessibilityLabel={`Chart: ${section.title}. Open the panel to read the numbers.`}>
            <ChartStrip section={section} width={width} height={compact ? 64 : 180} />
          </View>
        </View>
      );
    case 'table': {
      const columns = compact ? section.columns.slice(0, 2) : section.columns;
      const rows = section.rows.slice(0, limit);
      return (
        <View>
          <View style={[styles.tableRow, { borderBottomColor: t.borderPanel }]}>
            {columns.map((column, index) => (
              <Text key={index} numberOfLines={1} style={[typography.small, styles.tableCell, { color: t.muted, fontWeight: '600' }]}>{column}</Text>
            ))}
          </View>
          {rows.map((row, r) => (
            <View key={r} style={[styles.tableRow, { borderBottomColor: t.border }]}>
              {columns.map((_, c) => (
                <Text key={c} numberOfLines={compact ? 1 : undefined}
                  style={[typography.meta, styles.tableCell, { color: t.text, textAlign: section.column_types?.[c] === 'number' ? 'right' : 'left' }]}>
                  {row[c]}
                </Text>
              ))}
            </View>
          ))}
          <More count={section.rows.length - rows.length} />
          {compact && section.columns.length > 2 ? <Text style={[typography.small, { color: t.muted }]}>{section.columns.length - 2} more columns</Text> : null}
        </View>
      );
    }
    case 'collection':
      return (
        <View style={styles.gap}>
          {section.items.slice(0, limit).map(item => (
            <View key={item.id} style={styles.item}>
              <View style={styles.itemHead}>
                <Text style={[typography.body, { color: t.text, flex: 1 }]} numberOfLines={compact ? 1 : undefined}>{item.title}</Text>
                {item.status ? <Text style={[typography.small, { color: t.muted }]}>{item.status}</Text> : null}
              </View>
              {item.description && !compact ? <Text style={[typography.meta, { color: t.muted }]}>{item.description}</Text> : null}
            </View>
          ))}
          <More count={section.items.length - Math.min(section.items.length, limit)} />
        </View>
      );
    case 'progress':
      return (
        <View style={styles.gap}>
          {section.items.slice(0, limit).map(item => {
            const view = progressView(item);
            return (
              <View key={item.id} style={styles.gapSmall}>
                <View style={styles.itemHead}>
                  <Text style={[typography.meta, { color: t.text, flex: 1 }]}>{item.label}</Text>
                  <Text style={[typography.meta, { color: t.muted }]}>
                    {formatValue(item.actual, item.unit)} of {formatValue(item.target, item.unit)} · {view.percentage}
                  </Text>
                </View>
                <View style={[styles.track, { backgroundColor: t.selection }]}>
                  <View style={[styles.fill, { width: `${view.width}%`, backgroundColor: t.text }]} />
                </View>
              </View>
            );
          })}
          <More count={section.items.length - Math.min(section.items.length, limit)} />
        </View>
      );
    case 'timeline':
      return (
        <View style={styles.gap}>
          {section.events.slice(0, limit).map(event => (
            <View key={event.id} style={styles.itemHead}>
              <Text style={[typography.small, { color: t.muted, width: 64 }]}>{shortTime(event.at_ms / 1000)}</Text>
              <View style={{ flex: 1 }}>
                <Text style={[typography.meta, { color: t.text }]}>{event.label}</Text>
                {event.description && !compact ? <Text style={[typography.small, { color: t.muted }]}>{event.description}</Text> : null}
              </View>
            </View>
          ))}
          <More count={section.events.length - Math.min(section.events.length, limit)} />
        </View>
      );
    case 'pipeline':
      return (
        <View style={styles.grid}>
          {section.stages.map(stage => (
            <View key={stage.id} style={styles.cell}>
              <Text numberOfLines={1} style={[typography.small, { color: t.muted }]}>{stage.label}</Text>
              <Text style={[typography.header, { color: t.text }]}>{stage.count}</Text>
              {stage.value ? <Text style={[typography.small, { color: t.muted }]}>{stage.value}</Text> : null}
            </View>
          ))}
        </View>
      );
  }
}

const styles = StyleSheet.create({
  gap: { gap: 6 },
  gapSmall: { gap: 4 },
  big: { fontSize: 28, fontWeight: '700' },
  grid: { flexDirection: 'row', flexWrap: 'wrap', rowGap: 10 },
  cell: { width: '50%', paddingRight: 8, gap: 2 },
  tableRow: { flexDirection: 'row', borderBottomWidth: StyleSheet.hairlineWidth, paddingVertical: 6, gap: 8 },
  tableCell: { flex: 1 },
  item: { gap: 2 },
  itemHead: { flexDirection: 'row', alignItems: 'center', gap: 8 },
  track: { height: 6, borderRadius: 3, overflow: 'hidden' },
  fill: { height: 6, borderRadius: 3 },
});
