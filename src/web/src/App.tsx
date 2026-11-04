import { Navigate, Route, Routes } from "react-router";
import { Layout } from "./components/Layout";
import { Annotate } from "./pages/Annotate";
import { Articles } from "./pages/Articles";
import { DataPage } from "./pages/DataPage";
import { Overview } from "./pages/Overview";
import { Projects } from "./pages/Projects";
import { Review } from "./pages/Review";
import { Settings } from "./pages/Settings";

export function App() {
  return (
    <Routes>
      <Route element={<Layout />}>
        <Route index element={<Projects />} />
        <Route path="p/:pid" element={<Overview />} />
        <Route path="p/:pid/annotate" element={<Annotate />} />
        <Route path="p/:pid/annotate/:aid" element={<Annotate />} />
        <Route path="p/:pid/articles" element={<Articles />} />
        <Route path="p/:pid/review" element={<Review />} />
        <Route path="p/:pid/data" element={<DataPage />} />
        <Route path="settings" element={<Settings />} />
        <Route path="*" element={<Navigate to="/" replace />} />
      </Route>
    </Routes>
  );
}
