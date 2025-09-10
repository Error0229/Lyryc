import React from 'react';
import ReactDOM from 'react-dom/client';
import SettingsApp from './components/SettingsApp';
import './index.css';

const root = ReactDOM.createRoot(document.getElementById('settings-app') as HTMLElement);

root.render(
  <React.StrictMode>
    <SettingsApp />
  </React.StrictMode>
);