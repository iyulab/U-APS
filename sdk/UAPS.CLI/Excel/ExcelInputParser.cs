using ClosedXML.Excel;
using UAPS.SDK.Models;

namespace UAPS.CLI.Excel;

/// <summary>
/// 확장된 파싱 결과
/// </summary>
public class ParseResult
{
    public List<Job> Jobs { get; set; } = [];
    public List<Resource> Resources { get; set; } = [];
    public SkillMatrix? SkillMatrix { get; set; }
    public CertificationMatrix? CertificationMatrix { get; set; }
    public CrewManager? CrewManager { get; set; }
}

/// <summary>
/// Excel 입력 파일 파서
/// </summary>
public class ExcelInputParser
{
    /// <summary>
    /// 기본 파싱 (기존 호환성)
    /// </summary>
    public (List<Job> Jobs, List<Resource> Resources) Parse(string filePath)
    {
        using var workbook = new XLWorkbook(filePath);

        var jobs = ParseJobs(workbook);
        var operations = ParseOperations(workbook);
        var resources = ParseResources(workbook);

        // 공정을 Job에 연결
        LinkOperationsToJobs(jobs, operations);

        return (jobs, resources);
    }

    /// <summary>
    /// 확장 파싱 (인증, 팀, 스킬 정보 포함)
    /// </summary>
    public ParseResult ParseExtended(string filePath)
    {
        using var workbook = new XLWorkbook(filePath);

        var jobs = ParseJobs(workbook);
        var operations = ParseOperations(workbook);
        var resources = ParseResources(workbook);

        LinkOperationsToJobs(jobs, operations);

        return new ParseResult
        {
            Jobs = jobs,
            Resources = resources,
            SkillMatrix = ParseSkills(workbook),
            CertificationMatrix = ParseCertifications(workbook),
            CrewManager = ParseCrews(workbook)
        };
    }

    private List<Job> ParseJobs(XLWorkbook workbook)
    {
        var jobs = new List<Job>();
        var sheet = workbook.Worksheet("생산계획");

        if (sheet == null) return jobs;

        var rows = sheet.RangeUsed()?.RowsUsed().Skip(1) ?? Enumerable.Empty<IXLRangeRow>();

        foreach (var row in rows)
        {
            var orderId = row.Cell(1).GetString();
            if (string.IsNullOrWhiteSpace(orderId)) continue;

            var job = Job.Create(orderId)
                .WithPriority(row.Cell(4).GetValue<int>())
                .WithQuantity(row.Cell(3).GetValue<int>());

            // 납기일 파싱
            if (row.Cell(5).TryGetValue<DateTime>(out var dueDate))
            {
                job = job.WithDueDate(dueDate);
            }

            jobs.Add(job);
        }

        return jobs;
    }

    private List<Operation> ParseOperations(XLWorkbook workbook)
    {
        var operations = new List<Operation>();
        var sheet = workbook.Worksheet("공정정보");

        if (sheet == null) return operations;

        var rows = sheet.RangeUsed()?.RowsUsed().Skip(1) ?? Enumerable.Empty<IXLRangeRow>();

        foreach (var row in rows)
        {
            var opId = row.Cell(1).GetString();
            var jobId = row.Cell(2).GetString();

            if (string.IsNullOrWhiteSpace(opId) || string.IsNullOrWhiteSpace(jobId))
                continue;

            var sequence = row.Cell(3).GetValue<int>();
            var setupMinutes = row.Cell(5).GetValue<int>();
            var processMinutes = row.Cell(6).GetValue<int>();
            var waitMinutes = row.Cell(7).GetValue<int>();
            var equipmentIds = row.Cell(8).GetString()
                .Split(',', StringSplitOptions.RemoveEmptyEntries)
                .Select(s => s.Trim())
                .ToList();

            var operation = Operation.Create(opId, jobId, sequence)
                .WithTime(
                    setupMinutes * 60 * 1000,
                    processMinutes * 60 * 1000,
                    waitMinutes * 60 * 1000
                )
                .WithEquipment(equipmentIds.ToArray());

            operations.Add(operation);
        }

        return operations;
    }

    private List<Resource> ParseResources(XLWorkbook workbook)
    {
        var resources = new List<Resource>();

        // 설비 파싱
        var equipSheet = workbook.Worksheet("설비정보");
        if (equipSheet != null)
        {
            var rows = equipSheet.RangeUsed()?.RowsUsed().Skip(1) ?? Enumerable.Empty<IXLRangeRow>();

            foreach (var row in rows)
            {
                var id = row.Cell(1).GetString();
                if (string.IsNullOrWhiteSpace(id)) continue;

                var capability = row.Cell(3).GetString();
                var efficiency = row.Cell(4).TryGetValue<double>(out var eff) ? eff : 1.0;

                var resource = Resource.Equipment(id)
                    .WithCapability(capability)
                    .WithEfficiency(efficiency);

                resources.Add(resource);
            }
        }

        // 작업자 파싱
        var workerSheet = workbook.Worksheet("작업자정보");
        if (workerSheet != null)
        {
            var rows = workerSheet.RangeUsed()?.RowsUsed().Skip(1) ?? Enumerable.Empty<IXLRangeRow>();

            foreach (var row in rows)
            {
                var id = row.Cell(1).GetString();
                if (string.IsNullOrWhiteSpace(id)) continue;

                var efficiency = row.Cell(4).TryGetValue<double>(out var eff) ? eff : 1.0;

                var resource = Resource.Worker(id)
                    .WithEfficiency(efficiency);

                resources.Add(resource);
            }
        }

        return resources;
    }

    private void LinkOperationsToJobs(List<Job> jobs, List<Operation> operations)
    {
        var jobDict = jobs.ToDictionary(j => j.Id);

        foreach (var op in operations)
        {
            if (jobDict.TryGetValue(op.JobId, out var job))
            {
                job.WithOperation(op);
            }
        }
    }

    /// <summary>
    /// 스킬정보 시트 파싱
    /// 컬럼: 작업자ID, 공정유형, 숙련도
    /// </summary>
    private SkillMatrix? ParseSkills(XLWorkbook workbook)
    {
        var sheet = workbook.Worksheet("스킬정보");
        if (sheet == null) return null;

        var rows = sheet.RangeUsed()?.RowsUsed().Skip(1) ?? Enumerable.Empty<IXLRangeRow>();
        var hasData = false;
        var matrix = new SkillMatrix();

        foreach (var row in rows)
        {
            var workerId = row.Cell(1).GetString();
            var operationType = row.Cell(2).GetString();
            var proficiency = row.Cell(3).TryGetValue<double>(out var prof) ? prof : 0.5;

            if (string.IsNullOrWhiteSpace(workerId) || string.IsNullOrWhiteSpace(operationType))
                continue;

            matrix.WithSkill(workerId, operationType, proficiency);
            hasData = true;
        }

        return hasData ? matrix : null;
    }

    /// <summary>
    /// 인증정보 시트 파싱
    /// 컬럼: 인증ID, 작업자ID, 대상ID, 대상유형, 레벨, 발급일, 만료일
    /// </summary>
    private CertificationMatrix? ParseCertifications(XLWorkbook workbook)
    {
        var sheet = workbook.Worksheet("인증정보");
        if (sheet == null) return null;

        var rows = sheet.RangeUsed()?.RowsUsed().Skip(1) ?? Enumerable.Empty<IXLRangeRow>();
        var hasData = false;
        var matrix = new CertificationMatrix();

        foreach (var row in rows)
        {
            var certId = row.Cell(1).GetString();
            var workerId = row.Cell(2).GetString();
            var targetId = row.Cell(3).GetString();
            var targetTypeStr = row.Cell(4).GetString();
            var levelStr = row.Cell(5).GetString();

            if (string.IsNullOrWhiteSpace(certId) || string.IsNullOrWhiteSpace(workerId))
                continue;

            var target = ParseCertificationTarget(targetTypeStr);
            var level = ParseCertificationLevel(levelStr);

            // 발급일 (기본값: 현재)
            long issuedAtMs = DateTimeOffset.UtcNow.ToUnixTimeMilliseconds();
            if (row.Cell(6).TryGetValue<DateTime>(out var issuedDate))
            {
                issuedAtMs = new DateTimeOffset(issuedDate).ToUnixTimeMilliseconds();
            }

            var cert = Certification.Create(certId, workerId, targetId, target, level, issuedAtMs);

            // 만료일 (옵션)
            if (row.Cell(7).TryGetValue<DateTime>(out var expiryDate))
            {
                cert = cert.WithExpiry(new DateTimeOffset(expiryDate).ToUnixTimeMilliseconds());
            }

            matrix.WithCertification(cert);
            hasData = true;
        }

        return hasData ? matrix : null;
    }

    /// <summary>
    /// 팀정보 시트 파싱
    /// 컬럼: 팀ID, 팀명, 팀원IDs, 리더ID, 담당공정유형, 담당장비, 최소인원
    /// </summary>
    private CrewManager? ParseCrews(XLWorkbook workbook)
    {
        var sheet = workbook.Worksheet("팀정보");
        if (sheet == null) return null;

        var rows = sheet.RangeUsed()?.RowsUsed().Skip(1) ?? Enumerable.Empty<IXLRangeRow>();
        var hasData = false;
        var manager = new CrewManager();

        foreach (var row in rows)
        {
            var crewId = row.Cell(1).GetString();
            var crewName = row.Cell(2).GetString();

            if (string.IsNullOrWhiteSpace(crewId))
                continue;

            var crew = Crew.Create(crewId, crewName ?? crewId);

            // 팀원 IDs (콤마 구분)
            var memberIdsStr = row.Cell(3).GetString();
            if (!string.IsNullOrWhiteSpace(memberIdsStr))
            {
                var memberIds = memberIdsStr.Split(',', StringSplitOptions.RemoveEmptyEntries)
                    .Select(s => s.Trim()).ToArray();
                crew = crew.WithMembers(memberIds);
            }

            // 리더 ID
            var leaderId = row.Cell(4).GetString();
            if (!string.IsNullOrWhiteSpace(leaderId))
            {
                crew = crew.WithLeader(leaderId);
            }

            // 담당 공정유형 (콤마 구분)
            var opTypesStr = row.Cell(5).GetString();
            if (!string.IsNullOrWhiteSpace(opTypesStr))
            {
                foreach (var opType in opTypesStr.Split(',', StringSplitOptions.RemoveEmptyEntries))
                {
                    crew = crew.WithOperationType(opType.Trim());
                }
            }

            // 담당 장비 (콤마 구분)
            var equipIdsStr = row.Cell(6).GetString();
            if (!string.IsNullOrWhiteSpace(equipIdsStr))
            {
                foreach (var equipId in equipIdsStr.Split(',', StringSplitOptions.RemoveEmptyEntries))
                {
                    crew = crew.WithEquipment(equipId.Trim());
                }
            }

            // 최소 인원
            if (row.Cell(7).TryGetValue<int>(out var minMembers) && minMembers > 0)
            {
                crew = crew.WithMinMembers(minMembers);
            }

            manager.WithCrew(crew);
            hasData = true;
        }

        return hasData ? manager : null;
    }

    private static CertificationTarget ParseCertificationTarget(string targetType)
    {
        return targetType?.ToLowerInvariant() switch
        {
            "equipment" or "장비" => CertificationTarget.Equipment,
            "product" or "제품" => CertificationTarget.Product,
            _ => CertificationTarget.OperationType
        };
    }

    private static CertificationLevel ParseCertificationLevel(string level)
    {
        return level?.ToLowerInvariant() switch
        {
            "basic" or "기본" or "1" => CertificationLevel.Basic,
            "advanced" or "고급" or "2" => CertificationLevel.Advanced,
            "expert" or "전문가" or "3" => CertificationLevel.Expert,
            "master" or "마스터" or "4" => CertificationLevel.Master,
            _ => CertificationLevel.None
        };
    }
}
