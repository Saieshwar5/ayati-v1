import './style.css';

if (new URLSearchParams(location.search).get('view') === 'home') {
  void import('./home').then(({ installHome }) => installHome());
} else {
  void import('./chat');
}
