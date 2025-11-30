import React from "react";
import ReactDOM from "react-dom/client";
import { BrowserRouter } from "react-router-dom";
import App from "./App";
import { WalletContextProvider } from "./contexts/WalletContext";
import { DaoClientProvider } from "./contexts/DaoClientContext";
import "./index.css";

ReactDOM.createRoot(document.getElementById("root")!).render(
  <React.StrictMode>
    <BrowserRouter>
      <WalletContextProvider>
        <DaoClientProvider>
          <App />
        </DaoClientProvider>
      </WalletContextProvider>
    </BrowserRouter>
  </React.StrictMode>
);
