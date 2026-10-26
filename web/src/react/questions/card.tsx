import { useState } from 'react';
import { workspaceSession } from '../../workspace-context';
import { Button } from '../ui';
import { QuestionDialog } from './dialog';
import { questionState, type QuestionReference } from './model';
import './questions.css';

export function QuestionCards({ questions }: { questions: QuestionReference[] }) {
  const [opened, open] = useState<string>();
  const latest = questions.at(-1);
  if (!latest) return null;
  const card = (question: QuestionReference) => <section className="question-card" key={question.id} aria-label={`Question: ${question.title}`}>
    <strong>{question.title}</strong><small>{questionState(question.state)} · For {question.respondent_name}</small>
    <Button onClick={() => open(question.id)}>{question.state === 'pending' && question.respondent === workspaceSession?.user?.id ? 'Answer question' : 'View question'}</Button>
  </section>;
  return <>{card(latest)}{questions.length > 1 && <details className="question-history"><summary>Earlier questions ({questions.length - 1})</summary>{questions.slice(0, -1).map(card)}</details>}
    {opened && <QuestionDialog key={opened} id={opened} close={() => open(undefined)} />}</>;
}
