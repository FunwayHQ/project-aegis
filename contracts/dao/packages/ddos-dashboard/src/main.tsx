import React from 'react';
import ReactDOM from 'react-dom/client';
import { BrowserRouter } from 'react-router-dom';
import App from './App';
import { DDoSProvider } from './contexts/DDoSContext';
import './index.css';

ReactDOM.createRoot(document.getElementById('root')!).render(
  <React.StrictMode>
    <BrowserRouter>
      <DDoSProvider>
        <App />
      </DDoSProvider>
    </BrowserRouter>
  </React.StrictMode>
);
