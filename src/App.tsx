import { Island } from "./components/Island";
import { Kiko } from "./components/Kiko";

export default function App() {
  return (
    <main className="flex h-full items-center justify-center">
      <Island>
        <Kiko />
      </Island>
    </main>
  );
}
