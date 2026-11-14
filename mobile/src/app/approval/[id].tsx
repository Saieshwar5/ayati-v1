import { useLocalSearchParams } from 'expo-router';
import { ApprovalReview } from '../../decisions/approval-review';

export default function ApprovalSheet() {
  const { id } = useLocalSearchParams<{ id: string }>();
  return <ApprovalReview key={id} id={id} />;
}
