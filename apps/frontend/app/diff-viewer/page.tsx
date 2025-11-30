import { DiffViewer } from "@/components/blocks/diff-viewer/diff-viewer";

export default function Page() {
  return (
    <div className="container mx-auto p-4">
      <DiffViewer
        patch={`diff --git a/file.tsx b/file.tsx
index 1234567..2345678 100644
--- a/file.tsx
+++ b/file.tsx
@@ -1,1 +1,1 @@
-<div>Hello</div>
+<div>World</div>`}
      />
    </div>
  );
}
