import { HashRouter, Routes, Route } from "react-router-dom";
import { Sidebar } from "./components/AppShell";
import { ItemDetail } from "./components/ItemDetail";
import { ForYou, Search } from "./views/Browse";
import { Saved, Drops } from "./views/Boards";
import { Settings } from "./views/Settings";

export default function App() {
  return (
    <HashRouter>
      <div style={{ display: "flex", height: "100vh", overflow: "hidden" }}>
        <Sidebar />
        <div style={{ flex: 1, minWidth: 0, display: "flex", flexDirection: "column" }}>
          <Routes>
            <Route path="/" element={<ForYou />} />
            <Route path="/search" element={<Search />} />
            <Route path="/saved" element={<Saved />} />
            <Route path="/drops" element={<Drops />} />
            <Route path="/settings" element={<Settings />} />
          </Routes>
        </div>
        <ItemDetail />
      </div>
    </HashRouter>
  );
}
