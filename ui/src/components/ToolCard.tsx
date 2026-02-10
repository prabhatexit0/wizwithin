import type { ReactNode } from "react";

interface ToolCardProps {
  title: string;
  description: string;
  children: ReactNode;
}

export default function ToolCard({
  title,
  description,
  children,
}: ToolCardProps) {
  return (
    <div className="rounded-xl border border-zinc-700 bg-zinc-800/60 p-6 shadow-lg">
      <h2 className="text-lg font-semibold text-zinc-100 mb-1">{title}</h2>
      <p className="text-sm text-zinc-400 mb-4">{description}</p>
      {children}
    </div>
  );
}
