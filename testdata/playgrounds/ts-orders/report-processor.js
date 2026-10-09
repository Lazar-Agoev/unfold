export default async function processReport(job) {
  return { name: job.name, generatedAt: new Date().toISOString() };
}
