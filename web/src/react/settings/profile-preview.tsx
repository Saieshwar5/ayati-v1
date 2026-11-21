import { PersonFace, type PersonIdentity } from '../faces';
import { PresenceDot } from '../chat/presence-view';
import { Fact, ProfilePresence } from '../chat/profile-card';

/** Your profile as colleagues open it in Chat (profile-card.tsx), drawn from the
 * draft so it changes as you type. It has no actions: those belong to the viewer. */
export function ProfilePreview({ person, title, about, handles, email, hideOnline }: {
  person: PersonIdentity; title: string; about: string; handles: string; email: string; hideOnline: boolean;
}) {
  return <aside className="my-profile-preview" aria-label="How others see you">
    <p className="settings-group-title">How others see you</p>
    <div className="my-profile-preview-card profile-card">
      <div className="profile-card-identity">
        <PersonFace person={person} className="profile-card-face" badge={hideOnline ? undefined : <PresenceDot id={person.id} />} />
        <h3>{person.name}</h3>
        <p>{title}</p>
        <ProfilePresence id={person.id} name={person.name} hideOnline={hideOnline} />
      </div>
      <ul className="profile-card-facts">
        <Fact icon="about" label="About">{about || <span className="profile-card-none">Nothing written yet</span>}</Fact>
        <Fact icon="check" label="Handles">{handles || <span className="profile-card-none">Nothing written yet</span>}</Fact>
        <Fact icon="mail" label="Email">{email}</Fact>
      </ul>
    </div>
    <p className="ui-help">The card people open from your name in Chat. It changes as you type.</p>
  </aside>;
}
