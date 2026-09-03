using ClosedXML.Excel;
using UAPS.SDK.Client;
using UAPS.SDK.Models;

namespace UAPS.CLI.Excel;

/// <summary>
/// 스케줄링 결과 Excel 출력
/// </summary>
public class ExcelOutputWriter
{
    public void Write(Schedule schedule, List<Job> jobs, string outputPath)
    {
        using var workbook = new XLWorkbook();

        WriteScheduleSummary(workbook, schedule);
        WriteAssignments(workbook, schedule, jobs);
        WriteViolations(workbook, schedule);
        WriteGanttData(workbook, schedule);
        WriteKpiDashboard(workbook, schedule, jobs);

        workbook.SaveAs(outputPath);
    }

    private void WriteScheduleSummary(XLWorkbook workbook, Schedule schedule)
    {
        var sheet = workbook.AddWorksheet("스케줄요약");

        sheet.Cell(1, 1).Value = "항목";
        sheet.Cell(1, 2).Value = "값";

        sheet.Cell(2, 1).Value = "총 Makespan (분)";
        sheet.Cell(2, 2).Value = schedule.MakespanMs / 60000.0;

        sheet.Cell(3, 1).Value = "총 작업 수";
        sheet.Cell(3, 2).Value = schedule.Assignments.Count;

        sheet.Cell(4, 1).Value = "납기 준수";
        sheet.Cell(4, 2).Value = schedule.IsOnTime() ? "예" : "아니오";

        sheet.Cell(5, 1).Value = "위반 건수";
        sheet.Cell(5, 2).Value = schedule.Violations.Count;

        // 스타일 적용
        var headerRange = sheet.Range(1, 1, 1, 2);
        headerRange.Style.Font.Bold = true;
        headerRange.Style.Fill.BackgroundColor = XLColor.LightGray;

        sheet.Columns().AdjustToContents();
    }

    private void WriteAssignments(XLWorkbook workbook, Schedule schedule, List<Job> jobs)
    {
        var sheet = workbook.AddWorksheet("작업배정");

        // 헤더
        var headers = new[] { "작업ID", "공정ID", "순서", "자원ID", "시작시간", "종료시간", "소요시간(분)", "제품명" };
        for (int i = 0; i < headers.Length; i++)
        {
            sheet.Cell(1, i + 1).Value = headers[i];
        }

        // 데이터
        var jobDict = jobs.ToDictionary(j => j.Id);
        var sortedAssignments = schedule.Assignments
            .OrderBy(a => a.StartMs)
            .ThenBy(a => a.OperationId)
            .ToList();

        int row = 2;
        foreach (var assignment in sortedAssignments)
        {
            // Job ID 찾기
            var jobId = "";
            var sequence = 0;
            var productName = "";

            foreach (var job in jobs)
            {
                var op = job.Operations.FirstOrDefault(o => o.Id == assignment.OperationId);
                if (op != null)
                {
                    jobId = job.Id;
                    sequence = op.Sequence;
                    productName = job.ProductName ?? "";
                    break;
                }
            }

            sheet.Cell(row, 1).Value = jobId;
            sheet.Cell(row, 2).Value = assignment.OperationId;
            sheet.Cell(row, 3).Value = sequence;
            sheet.Cell(row, 4).Value = assignment.ResourceId;
            sheet.Cell(row, 5).Value = FormatTime(assignment.StartMs);
            sheet.Cell(row, 6).Value = FormatTime(assignment.EndMs);
            sheet.Cell(row, 7).Value = (assignment.EndMs - assignment.StartMs) / 60000.0;
            sheet.Cell(row, 8).Value = productName;

            row++;
        }

        // 스타일
        var headerRange = sheet.Range(1, 1, 1, headers.Length);
        headerRange.Style.Font.Bold = true;
        headerRange.Style.Fill.BackgroundColor = XLColor.LightGray;

        sheet.Columns().AdjustToContents();
    }

    private void WriteViolations(XLWorkbook workbook, Schedule schedule)
    {
        var sheet = workbook.AddWorksheet("위반사항");

        // 헤더
        var headers = new[] { "유형", "대상ID", "위반량(분)", "설명" };
        for (int i = 0; i < headers.Length; i++)
        {
            sheet.Cell(1, i + 1).Value = headers[i];
        }

        // 데이터
        int row = 2;
        foreach (var violation in schedule.Violations)
        {
            sheet.Cell(row, 1).Value = violation.ConstraintType;
            sheet.Cell(row, 2).Value = violation.TargetId;
            sheet.Cell(row, 3).Value = violation.Amount / 60000.0;
            sheet.Cell(row, 4).Value = violation.Description;
            row++;
        }

        // 스타일
        var headerRange = sheet.Range(1, 1, 1, headers.Length);
        headerRange.Style.Font.Bold = true;
        headerRange.Style.Fill.BackgroundColor = XLColor.LightGray;

        if (schedule.Violations.Count > 0)
        {
            var dataRange = sheet.Range(2, 1, row - 1, headers.Length);
            dataRange.Style.Fill.BackgroundColor = XLColor.LightPink;
        }

        sheet.Columns().AdjustToContents();
    }

    private void WriteGanttData(XLWorkbook workbook, Schedule schedule)
    {
        var sheet = workbook.AddWorksheet("간트차트데이터");

        // 헤더
        var headers = new[] { "자원ID", "공정ID", "시작(분)", "종료(분)", "길이(분)" };
        for (int i = 0; i < headers.Length; i++)
        {
            sheet.Cell(1, i + 1).Value = headers[i];
        }

        // 자원별 그룹화
        var byResource = schedule.Assignments
            .GroupBy(a => a.ResourceId)
            .OrderBy(g => g.Key);

        int row = 2;
        foreach (var group in byResource)
        {
            foreach (var assignment in group.OrderBy(a => a.StartMs))
            {
                sheet.Cell(row, 1).Value = group.Key;
                sheet.Cell(row, 2).Value = assignment.OperationId;
                sheet.Cell(row, 3).Value = assignment.StartMs / 60000.0;
                sheet.Cell(row, 4).Value = assignment.EndMs / 60000.0;
                sheet.Cell(row, 5).Value = (assignment.EndMs - assignment.StartMs) / 60000.0;
                row++;
            }
        }

        // 스타일
        var headerRange = sheet.Range(1, 1, 1, headers.Length);
        headerRange.Style.Font.Bold = true;
        headerRange.Style.Fill.BackgroundColor = XLColor.LightGray;

        sheet.Columns().AdjustToContents();
    }

    private string FormatTime(long ms)
    {
        var totalMinutes = ms / 60000;
        var hours = totalMinutes / 60;
        var minutes = totalMinutes % 60;
        return $"{hours:D2}:{minutes:D2}";
    }

    /// <summary>
    /// KPI 대시보드 시트 작성
    /// </summary>
    private void WriteKpiDashboard(XLWorkbook workbook, Schedule schedule, List<Job> jobs)
    {
        var sheet = workbook.AddWorksheet("KPI대시보드");

        // 헤더 및 스타일 설정
        sheet.Cell(1, 1).Value = "KPI 대시보드";
        sheet.Cell(1, 1).Style.Font.Bold = true;
        sheet.Cell(1, 1).Style.Font.FontSize = 16;

        // 기본 KPI 섹션
        WriteKpiSection(sheet, 3, "기본 성과 지표", new Dictionary<string, object>
        {
            ["총 Makespan (분)"] = Math.Round(schedule.MakespanMs / 60000.0, 2),
            ["총 작업 수"] = schedule.Assignments.Count,
            ["총 Job 수"] = jobs.Count,
            ["납기 준수율 (%)"] = CalculateOnTimeRate(schedule, jobs)
        });

        // 자원 활용률 섹션
        var resourceUtilization = CalculateResourceUtilization(schedule);
        WriteKpiSection(sheet, 9, "자원 활용률", resourceUtilization);

        // 납기 관련 KPI
        var dueDateKpis = CalculateDueDateKpis(schedule, jobs);
        WriteKpiSection(sheet, 9 + resourceUtilization.Count + 2, "납기 성과", dueDateKpis);

        sheet.Columns().AdjustToContents();
    }

    private void WriteKpiSection(IXLWorksheet sheet, int startRow, string title, Dictionary<string, object> kpis)
    {
        sheet.Cell(startRow, 1).Value = title;
        sheet.Cell(startRow, 1).Style.Font.Bold = true;
        sheet.Cell(startRow, 1).Style.Fill.BackgroundColor = XLColor.LightBlue;
        sheet.Range(startRow, 1, startRow, 2).Merge();

        int row = startRow + 1;
        foreach (var kpi in kpis)
        {
            sheet.Cell(row, 1).Value = kpi.Key;
            sheet.Cell(row, 2).Value = kpi.Value?.ToString() ?? "-";
            row++;
        }
    }

    private double CalculateOnTimeRate(Schedule schedule, List<Job> jobs)
    {
        if (jobs.Count == 0) return 100.0;

        int onTimeCount = 0;
        foreach (var job in jobs)
        {
            if (!job.DueDate.HasValue)
            {
                onTimeCount++;
                continue;
            }

            var dueMs = new DateTimeOffset(job.DueDate.Value).ToUnixTimeMilliseconds();
            var lastOp = job.Operations.OrderByDescending(o => o.Sequence).FirstOrDefault();
            if (lastOp == null)
            {
                onTimeCount++;
                continue;
            }

            var assignment = schedule.GetAssignmentForOperation(lastOp.Id);
            if (assignment == null || assignment.EndMs <= dueMs)
            {
                onTimeCount++;
            }
        }

        return Math.Round(onTimeCount * 100.0 / jobs.Count, 1);
    }

    private Dictionary<string, object> CalculateResourceUtilization(Schedule schedule)
    {
        var result = new Dictionary<string, object>();
        if (schedule.Assignments.Count == 0 || schedule.MakespanMs == 0)
        {
            result["데이터 없음"] = "-";
            return result;
        }

        var byResource = schedule.Assignments.GroupBy(a => a.ResourceId);
        double totalUtilization = 0;
        int resourceCount = 0;

        foreach (var group in byResource)
        {
            var totalWorkMs = group.Sum(a => a.EndMs - a.StartMs);
            var utilization = Math.Round(totalWorkMs * 100.0 / schedule.MakespanMs, 1);
            result[$"{group.Key} 활용률 (%)"] = utilization;
            totalUtilization += utilization;
            resourceCount++;
        }

        if (resourceCount > 0)
        {
            result["평균 활용률 (%)"] = Math.Round(totalUtilization / resourceCount, 1);
        }

        return result;
    }

    private Dictionary<string, object> CalculateDueDateKpis(Schedule schedule, List<Job> jobs)
    {
        var result = new Dictionary<string, object>();

        var jobsWithDueDate = jobs.Where(j => j.DueDate.HasValue).ToList();
        if (jobsWithDueDate.Count == 0)
        {
            result["납기 설정 Job 없음"] = "-";
            return result;
        }

        int lateCount = 0;
        double totalTardiness = 0;
        double maxTardiness = 0;

        foreach (var job in jobsWithDueDate)
        {
            var dueMs = new DateTimeOffset(job.DueDate!.Value).ToUnixTimeMilliseconds();
            var lastOp = job.Operations.OrderByDescending(o => o.Sequence).FirstOrDefault();
            if (lastOp == null) continue;

            var assignment = schedule.GetAssignmentForOperation(lastOp.Id);
            if (assignment == null) continue;

            if (assignment.EndMs > dueMs)
            {
                lateCount++;
                var tardiness = (assignment.EndMs - dueMs) / 60000.0; // 분 단위
                totalTardiness += tardiness;
                if (tardiness > maxTardiness) maxTardiness = tardiness;
            }
        }

        result["납기 대상 Job 수"] = jobsWithDueDate.Count;
        result["지연 Job 수"] = lateCount;
        result["총 지연 시간 (분)"] = Math.Round(totalTardiness, 1);
        result["최대 지연 시간 (분)"] = Math.Round(maxTardiness, 1);
        result["평균 지연 시간 (분)"] = lateCount > 0 ? Math.Round(totalTardiness / lateCount, 1) : 0;

        return result;
    }
}
