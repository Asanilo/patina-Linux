import { lazy, Suspense, type ComponentProps } from "react";

const LazyDestinationDetailDialog = lazy(() => import("./DestinationDetailDialog.tsx"));

type Props = ComponentProps<
  typeof import("./DestinationDetailDialog.tsx")["default"]
>;

export default function DestinationDetailDialogEntry(props: Props) {
  return (
    <Suspense fallback={null}>
      <LazyDestinationDetailDialog {...props} />
    </Suspense>
  );
}
