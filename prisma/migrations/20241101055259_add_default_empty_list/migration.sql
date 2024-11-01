-- AlterTable
ALTER TABLE "processed_email" ALTER COLUMN "labels_applied" SET DEFAULT ARRAY[]::TEXT[],
ALTER COLUMN "labels_removed" SET DEFAULT ARRAY[]::TEXT[];
