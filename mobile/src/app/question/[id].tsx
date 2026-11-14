import { useLocalSearchParams } from 'expo-router';
import { QuestionReview } from '../../decisions/question-review';

export default function QuestionSheet() {
  const { id } = useLocalSearchParams<{ id: string }>();
  return <QuestionReview key={id} id={id} />;
}
