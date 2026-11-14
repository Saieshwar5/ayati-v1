import { render, screen } from '@testing-library/react-native';
import { SectionView } from '../src/home/section-view';

describe('panel sections on a phone', () => {
  it('shrinks a wide table to two columns and says how much more there is', async () => {
    await render(
      <SectionView
        compact
        section={{
          kind: 'table',
          columns: ['Customer', 'Orders', 'Revenue', 'Region'],
          rows: [
            ['Asha', '4', '€120', 'North'],
            ['Ben', '2', '€80', 'South'],
            ['Chen', '7', '€300', 'East'],
            ['Dara', '1', '€20', 'West'],
            ['Eli', '3', '€95', 'North'],
          ],
          column_types: ['text', 'number', 'number', 'text'],
        }}
      />,
    );
    expect(screen.getByText('Customer')).toBeTruthy();
    expect(screen.getByText('Orders')).toBeTruthy();
    expect(screen.queryByText('Revenue')).toBeNull();
    expect(screen.getByText('2 more')).toBeTruthy();
    expect(screen.getByText('2 more columns')).toBeTruthy();
    expect(screen.queryByText('Dara')).toBeNull();
  });

  it('shows the whole table on the panel page, and progress against its target', async () => {
    await render(
      <>
        <SectionView section={{ kind: 'table', columns: ['A', 'B', 'C'], rows: [['1', '2', '3'], ['4', '5', '6'], ['7', '8', '9'], ['x', 'y', 'z']] }} />
        <SectionView section={{ kind: 'progress', items: [{ id: 'p', label: 'Monthly sales', actual: 50, target: 200, unit: '€' }] }} />
      </>,
    );
    expect(screen.getByText('C')).toBeTruthy();
    expect(screen.getByText('z')).toBeTruthy();
    expect(screen.getByText('50 € of 200 € · 25%')).toBeTruthy();
  });

  it('labels a chart so a screen reader user knows where its numbers are', async () => {
    await render(<SectionView compact section={{ kind: 'chart', title: 'Sales this week', points: [{ label: 'Mon', value: 3 }] }} />);
    expect(screen.getByLabelText('Chart: Sales this week. Open the panel to read the numbers.')).toBeTruthy();
  });
});
