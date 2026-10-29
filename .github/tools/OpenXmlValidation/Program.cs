using DocumentFormat.OpenXml;
using DocumentFormat.OpenXml.Packaging;
using DocumentFormat.OpenXml.Validation;
using Presentation = DocumentFormat.OpenXml.Presentation;

if (args.Length == 0)
{
    Console.Error.WriteLine("Usage: OpenXmlValidation <file.docx|file.pptx|file.xlsx|directory> [...]");
    return 2;
}

var failed = false;
var count = 0;
var validator = new OpenXmlValidator(FileFormatVersions.Microsoft365) { MaxNumberOfErrors = 0 };
foreach (var input in args)
{
    try
    {
        var files = Directory.Exists(input)
            ? Directory.GetFiles(input).Where(IsOfficeFile).Order().ToArray()
            : new[] { input };
        if (files.Length == 0)
            throw new InvalidDataException($"No Office files found in {input}");

        foreach (var file in files)
        {
            count++;
            try
            {
                using OpenXmlPackage document = Path.GetExtension(file).ToLowerInvariant() switch
                {
                    ".docx" => WordprocessingDocument.Open(file, false),
                    ".pptx" => PresentationDocument.Open(file, false),
                    ".xlsx" => SpreadsheetDocument.Open(file, false),
                    _ => throw new InvalidDataException($"Unsupported file: {file}"),
                };
                var errors = validator.Validate(document).ToArray();
                foreach (var error in errors)
                    Console.Error.WriteLine(
                        $"{file} | {error.Part?.Uri} | {error.Path?.XPath} | {error.Id}: {error.Description}");
                var identityErrors = document is PresentationDocument presentation
                    ? ValidatePresentationIds(presentation).ToArray()
                    : Array.Empty<string>();
                foreach (var error in identityErrors)
                    Console.Error.WriteLine($"{file} | {error}");
                Console.WriteLine($"{file}: {errors.Length + identityErrors.Length} validation error(s)");
                failed |= errors.Length + identityErrors.Length > 0;
            }
            catch (Exception error) when (error is IOException or OpenXmlPackageException
                or System.Xml.XmlException or ArgumentException or InvalidOperationException
                or UnauthorizedAccessException or FormatException)
            {
                Console.Error.WriteLine($"{file}: {error.GetType().Name}: {error.Message}");
                failed = true;
            }
        }
    }
    catch (Exception error) when (error is IOException or OpenXmlPackageException
        or System.Xml.XmlException or ArgumentException or InvalidOperationException)
    {
        Console.Error.WriteLine($"{input}: {error.GetType().Name}: {error.Message}");
        failed = true;
    }
}
Console.WriteLine($"Validated {count} Office file(s) with Open XML SDK (Microsoft365).");
return failed ? 1 : 0;

static bool IsOfficeFile(string path) =>
    !Path.GetFileName(path).StartsWith("~$", StringComparison.Ordinal)
    && Path.GetExtension(path).ToLowerInvariant() is ".docx" or ".pptx" or ".xlsx";

// PowerPoint repairs master/layout ID collisions even when the SDK schema passes.
static IEnumerable<string> ValidatePresentationIds(PresentationDocument document)
{
    var part = document.PresentationPart;
    if (part is null)
        yield break;
    var ids = new HashSet<uint>();
    foreach (var master in part.Presentation?.SlideMasterIdList?
        .Elements<Presentation.SlideMasterId>() ?? [])
    {
        if (master.Id?.Value is uint id && !ids.Add(id))
            yield return $"{part.Uri}: duplicate master/layout ID {id}";
    }
    foreach (var master in part.SlideMasterParts)
    {
        foreach (var layout in master.SlideMaster?.SlideLayoutIdList?
            .Elements<Presentation.SlideLayoutId>() ?? [])
        {
            if (layout.Id?.Value is uint id && !ids.Add(id))
                yield return $"{master.Uri}: duplicate master/layout ID {id}";
        }
    }
}
