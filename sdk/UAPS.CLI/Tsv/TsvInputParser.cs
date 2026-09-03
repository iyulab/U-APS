using UAPS.SDK.Models;

namespace UAPS.CLI.Tsv;

/// <summary>
/// TSV (Tab-Separated Values) input file parser
///
/// Expected file format:
/// - First row: header with column names
/// - Subsequent rows: data
/// - Columns separated by tabs
///
/// File naming convention:
/// - jobs.tsv: Job definitions
/// - resources.tsv: Resource definitions
/// - operations.tsv: Operation definitions
/// </summary>
public class TsvInputParser
{
    /// <summary>
    /// Parse TSV files from a directory
    /// </summary>
    /// <param name="directoryPath">Directory containing TSV files</param>
    /// <returns>Parsed jobs and resources</returns>
    public (List<Job> Jobs, List<Resource> Resources) ParseDirectory(string directoryPath)
    {
        if (!Directory.Exists(directoryPath))
        {
            throw new DirectoryNotFoundException($"TSV directory not found: {directoryPath}");
        }

        var jobs = new List<Job>();
        var resources = new List<Resource>();
        var operationsMap = new Dictionary<string, List<Operation>>();

        // Parse resources first
        var resourcesFile = Path.Combine(directoryPath, "resources.tsv");
        if (File.Exists(resourcesFile))
        {
            resources = ParseResourcesFile(resourcesFile);
        }

        // Parse operations (may reference resources)
        var operationsFile = Path.Combine(directoryPath, "operations.tsv");
        if (File.Exists(operationsFile))
        {
            operationsMap = ParseOperationsFile(operationsFile);
        }

        // Parse jobs and attach operations
        var jobsFile = Path.Combine(directoryPath, "jobs.tsv");
        if (File.Exists(jobsFile))
        {
            jobs = ParseJobsFile(jobsFile, operationsMap);
        }

        return (jobs, resources);
    }

    /// <summary>
    /// Parse a single TSV file containing all data
    /// Format: First column determines row type (JOB, OP, RES)
    /// </summary>
    public (List<Job> Jobs, List<Resource> Resources) ParseFile(string filePath)
    {
        if (!File.Exists(filePath))
        {
            throw new FileNotFoundException($"TSV file not found: {filePath}");
        }

        var lines = File.ReadAllLines(filePath)
            .Where(l => !string.IsNullOrWhiteSpace(l))
            .ToList();

        if (lines.Count == 0)
        {
            return ([], []);
        }

        var jobs = new List<Job>();
        var resources = new List<Resource>();
        var operationsMap = new Dictionary<string, List<Operation>>();

        string? currentSection = null;
        string[]? currentHeaders = null;

        foreach (var line in lines)
        {
            var trimmedLine = line.Trim();

            // Skip comments
            if (trimmedLine.StartsWith('#'))
            {
                continue;
            }

            // Section markers
            if (trimmedLine.StartsWith("[") && trimmedLine.EndsWith("]"))
            {
                currentSection = trimmedLine[1..^1].ToUpperInvariant();
                currentHeaders = null;
                continue;
            }

            var columns = trimmedLine.Split('\t');

            // First data row after section marker is the header
            if (currentHeaders == null)
            {
                currentHeaders = columns;
                continue;
            }

            // Parse data based on section
            switch (currentSection)
            {
                case "JOBS":
                    jobs.Add(ParseJobRow(columns, currentHeaders));
                    break;
                case "RESOURCES":
                    resources.Add(ParseResourceRow(columns, currentHeaders));
                    break;
                case "OPERATIONS":
                    var op = ParseOperationRow(columns, currentHeaders);
                    if (!operationsMap.ContainsKey(op.JobId))
                    {
                        operationsMap[op.JobId] = [];
                    }
                    operationsMap[op.JobId].Add(op);
                    break;
            }
        }

        // Attach operations to jobs
        foreach (var job in jobs)
        {
            if (operationsMap.TryGetValue(job.Id, out var ops))
            {
                job.Operations = ops.OrderBy(o => o.Sequence).ToList();
            }
        }

        return (jobs, resources);
    }

    private List<Job> ParseJobsFile(string filePath, Dictionary<string, List<Operation>> operationsMap)
    {
        var lines = File.ReadAllLines(filePath)
            .Where(l => !string.IsNullOrWhiteSpace(l) && !l.TrimStart().StartsWith('#'))
            .ToList();

        if (lines.Count < 2) return [];

        var headers = lines[0].Split('\t');
        var jobs = new List<Job>();

        for (int i = 1; i < lines.Count; i++)
        {
            var columns = lines[i].Split('\t');
            var job = ParseJobRow(columns, headers);

            if (operationsMap.TryGetValue(job.Id, out var ops))
            {
                job.Operations = ops.OrderBy(o => o.Sequence).ToList();
            }

            jobs.Add(job);
        }

        return jobs;
    }

    private List<Resource> ParseResourcesFile(string filePath)
    {
        var lines = File.ReadAllLines(filePath)
            .Where(l => !string.IsNullOrWhiteSpace(l) && !l.TrimStart().StartsWith('#'))
            .ToList();

        if (lines.Count < 2) return [];

        var headers = lines[0].Split('\t');
        var resources = new List<Resource>();

        for (int i = 1; i < lines.Count; i++)
        {
            var columns = lines[i].Split('\t');
            resources.Add(ParseResourceRow(columns, headers));
        }

        return resources;
    }

    private Dictionary<string, List<Operation>> ParseOperationsFile(string filePath)
    {
        var lines = File.ReadAllLines(filePath)
            .Where(l => !string.IsNullOrWhiteSpace(l) && !l.TrimStart().StartsWith('#'))
            .ToList();

        if (lines.Count < 2) return [];

        var headers = lines[0].Split('\t');
        var operationsMap = new Dictionary<string, List<Operation>>();

        for (int i = 1; i < lines.Count; i++)
        {
            var columns = lines[i].Split('\t');
            var op = ParseOperationRow(columns, headers);

            if (!operationsMap.ContainsKey(op.JobId))
            {
                operationsMap[op.JobId] = [];
            }
            operationsMap[op.JobId].Add(op);
        }

        return operationsMap;
    }

    private Job ParseJobRow(string[] columns, string[] headers)
    {
        var job = new Job();

        for (int i = 0; i < Math.Min(columns.Length, headers.Length); i++)
        {
            var header = headers[i].Trim().ToUpperInvariant();
            var value = columns[i].Trim();

            switch (header)
            {
                case "ID":
                case "JOB_ID":
                case "JOBID":
                    job.Id = value;
                    break;
                case "PRODUCT":
                case "PRODUCT_NAME":
                case "PRODUCTNAME":
                    job.ProductName = value;
                    break;
                case "PRIORITY":
                    if (int.TryParse(value, out var priority))
                        job.Priority = priority;
                    break;
                case "QUANTITY":
                case "QTY":
                    if (int.TryParse(value, out var qty))
                        job.Quantity = qty;
                    break;
                case "DUE_DATE":
                case "DUEDATE":
                case "DUE":
                    if (DateTime.TryParse(value, out var dueDate))
                        job.DueDate = dueDate;
                    break;
            }
        }

        return job;
    }

    private Resource ParseResourceRow(string[] columns, string[] headers)
    {
        var resource = new Resource();

        for (int i = 0; i < Math.Min(columns.Length, headers.Length); i++)
        {
            var header = headers[i].Trim().ToUpperInvariant();
            var value = columns[i].Trim();

            switch (header)
            {
                case "ID":
                case "RESOURCE_ID":
                case "RESOURCEID":
                    resource.Id = value;
                    break;
                case "TYPE":
                case "KIND":
                case "RESOURCE_TYPE":
                    resource.Kind = ParseResourceKind(value);
                    break;
                case "CAPABILITY":
                case "CAPABILITIES":
                    if (!string.IsNullOrEmpty(value))
                    {
                        resource.Capabilities = value.Split(',')
                            .Select(c => c.Trim())
                            .Where(c => !string.IsNullOrEmpty(c))
                            .ToList();
                    }
                    break;
                case "EFFICIENCY":
                    if (double.TryParse(value, out var efficiency))
                        resource.Efficiency = efficiency;
                    break;
                case "CALENDAR_ID":
                case "CALENDARID":
                    resource.CalendarId = value;
                    break;
                case "LINKED_RESOURCE":
                case "LINKED":
                    resource.LinkedResourceId = value;
                    break;
            }
        }

        return resource;
    }

    private Operation ParseOperationRow(string[] columns, string[] headers)
    {
        var operation = new Operation();
        var resourceReqs = new List<ResourceRequirement>();

        for (int i = 0; i < Math.Min(columns.Length, headers.Length); i++)
        {
            var header = headers[i].Trim().ToUpperInvariant();
            var value = columns[i].Trim();

            switch (header)
            {
                case "ID":
                case "OPERATION_ID":
                case "OPERATIONID":
                case "OP_ID":
                    operation.Id = value;
                    break;
                case "JOB_ID":
                case "JOBID":
                case "JOB":
                    operation.JobId = value;
                    break;
                case "SEQUENCE":
                case "SEQ":
                case "ORDER":
                    if (int.TryParse(value, out var seq))
                        operation.Sequence = seq;
                    break;
                case "OPERATION_TYPE":
                case "TYPE":
                case "OP_TYPE":
                    operation.OperationType = value;
                    break;
                case "SETUP_MS":
                case "SETUP":
                    if (long.TryParse(value, out var setup))
                        operation.Time = operation.Time with { SetupMs = setup };
                    break;
                case "PROCESS_MS":
                case "PROCESS":
                    if (long.TryParse(value, out var process))
                        operation.Time = operation.Time with { ProcessMs = process };
                    break;
                case "WAIT_MS":
                case "WAIT":
                    if (long.TryParse(value, out var wait))
                        operation.Time = operation.Time with { WaitMs = wait };
                    break;
                case "EQUIPMENT":
                case "EQUIPMENT_ID":
                case "MACHINE":
                    if (!string.IsNullOrEmpty(value))
                    {
                        resourceReqs.Add(new ResourceRequirement
                        {
                            ResourceType = ResourceType.Equipment,
                            Quantity = 1,
                            Candidates = value.Split(',').Select(c => c.Trim()).ToList()
                        });
                    }
                    break;
                case "WORKER":
                case "WORKERS":
                case "WORKER_COUNT":
                    if (int.TryParse(value, out var workerCount) && workerCount > 0)
                    {
                        resourceReqs.Add(new ResourceRequirement
                        {
                            ResourceType = ResourceType.Worker,
                            Quantity = workerCount,
                            Candidates = []
                        });
                    }
                    break;
                case "REQUIRES_OPERATOR":
                case "OPERATOR":
                    operation.RequiresOperator = value.ToUpperInvariant() is "TRUE" or "YES" or "1" or "Y";
                    break;
            }
        }

        if (resourceReqs.Count > 0)
        {
            operation.RequiredResources = resourceReqs;
        }

        return operation;
    }

    private static ResourceKind ParseResourceKind(string value)
    {
        return value.ToUpperInvariant() switch
        {
            "WORKER" or "W" => ResourceKind.Worker,
            "EQUIPMENT" or "E" or "MACHINE" or "M" => ResourceKind.Equipment,
            _ => ResourceKind.Equipment
        };
    }
}
