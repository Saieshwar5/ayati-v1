import { messageHtml } from '../../markdown';
import { PanelChart } from '../home-panels/chart';
import '../../panel-visualizations.css';
import type { PageBlock } from './page-format';

export function PageBlocks({ blocks }: { blocks: PageBlock[] }) {
  return <>{blocks.map((block, index) => {
    if (block.kind === 'markdown') return <div key={index} className="message-markdown page-text" dangerouslySetInnerHTML={{ __html: messageHtml(block.text) }} />;
    if (block.kind === 'chart') return <figure key={index} className="page-figure panel-chart"><PanelChart section={block.section} /></figure>;
    if (block.kind === 'steps') return <figure key={index} className="page-figure">
      <ol className="page-steps">{block.steps.map((step, number) => <li key={number}>
        <strong>{number + 1} · {step.title}</strong>{step.detail && <span>{step.detail}</span>}
      </li>)}</ol>
    </figure>;
    return <figure key={index} className="page-figure"><figcaption>{block.label}</figcaption><pre>{block.text}</pre></figure>;
  })}</>;
}
