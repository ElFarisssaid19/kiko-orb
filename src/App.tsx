import { Island } from "./components/Island";
import { Kiko } from "./components/Kiko";

export default function App() {
  return (
    <main className="flex h-full items-center justify-center">
      <Island>
        <div className="flex items-center gap-2">
          <Kiko mood="idle" size={28} />
          <span className="text-lg font-semibold">Kiko</span>
        </div>
      </Island>
    </main>
  );
}
