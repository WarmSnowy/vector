import algoliasearch, { SearchIndex } from "algoliasearch";
import chalk from "chalk";
import cheerio from "cheerio";
import { Element } from "domhandler";
import dotEnv from "dotenv-defaults";
import fs from "fs";
import glob from "glob-promise";
import chunk from "lodash.chunk";
import path from "path";

type Payload = {
  level: number;
  domId?: string;
  tagName: string;
  content: string;
}[];

type AlgoliaRecord = {
  objectID: string;
  pageUrl: string;
  itemUrl: string;
  level: number;
  title: string;
  tags: string[];
  ranking: number;
  section: string;
  content: string;
};

const algoliaIndexName = process.env.ALGOLIA_INDEX_NAME;
const algoliaBatchSize = 100;
const publicPath = path.resolve(__dirname, "../..", "public");
const tagHierarchy = {
  h1: 6,
  h2: 5,
  h3: 4,
  h4: 3,
  h5: 2,
  h6: 1,
  li: 1,
  p: 1,
};

function getPageUrl(file: string) {
  const filePath = file.split("public/")[1].split(path.sep).slice(0, -1);
  return `/${filePath.join("/")}`;
}

function getItemUrl(file: string, { level, domId }: Payload[0]) {
  const fileUrl = getPageUrl(file);

  if (level > 1 && level < 6 && !domId) {
    console.log(chalk.yellow(`Missing domId for level ${level}`));
    console.log(chalk.yellow(`File ${file}`));
  }

  return level > 1 && level < 6 && domId ? `${fileUrl}#${domId}` : fileUrl;
}

async function indexHTMLFiles(
  index: SearchIndex | null,
  section: string,
  files: string[],
  ranking: number
) {
  const usedIds = {};

  for (const file of files) {
    const html = fs.readFileSync(file, "utf-8");
    const $ = cheerio.load(html);
    const containers = $(".algolia-container");
    $(".algolia-no-index").each((_, d) => $(d).remove());
    $(".highlight").each((_, d) => $(d).remove());
    const payload: Payload = [];
    const traverse = (node?: Element) => {
      if (!node) {
        return;
      }

      console.log(node.tagName);

      const level = tagHierarchy[node.tagName];

      if (level) {
        payload.push({
          level,
          domId: $(node).attr("id"),
          tagName: node.tagName,
          content: $(node)
            .text()
            .replace(/[\n\t]/g, " "),
        });
      }

      $(node)
        .children()
        .map((_, d) => traverse(d));
    };

    for (let i = 0; i < containers.length; i++) {
      traverse(containers.get(i) as Element);
    }

    const algoliaRecords: AlgoliaRecord[] = [];

    let activeRecord: AlgoliaRecord | null = null;

    for (const item of payload) {
      const pageUrl = getPageUrl(file);
      const itemUrl = getItemUrl(file, item);

      if (!activeRecord) {
        activeRecord = {
          objectID: itemUrl,
          pageUrl,
          itemUrl,
          level: item.level,
          title: item.content,
          section,
          ranking,
          tags: [],
          content: "",
        };
      } else if (item.level === 1) {
        if (activeRecord.content) {
          activeRecord.content += " ";
        }

        activeRecord.content += item.content;
      } else if (item.level < activeRecord.level) {
        algoliaRecords.push({ ...activeRecord });

        activeRecord = {
          objectID: itemUrl,
          pageUrl,
          itemUrl,
          level: item.level,
          title: item.content,
          section,
          ranking,
          tags: [...activeRecord.tags, activeRecord.title],
          content: "",
        };
      } else {
        algoliaRecords.push({ ...activeRecord });
        const tagCount = activeRecord.tags.length;
        const levelDiff = item.level - activeRecord.level;
        const lastIndex = tagCount - levelDiff;

        activeRecord = {
          objectID: itemUrl,
          pageUrl,
          itemUrl,
          level: item.level,
          title: item.content,
          section,
          ranking,
          tags: [...activeRecord.tags.slice(0, lastIndex)],
          content: "",
        };
      }

      if (activeRecord) {
        algoliaRecords.push({ ...activeRecord });
      }

      for (const rec of algoliaRecords) {
        if (usedIds[rec.objectID]) {
          // The objectID is the url of the section of the page that the record covers.
          // If you have a duplicate here somehow two records point to the same thing.
          console.log(chalk.yellow(`Duplicate ID for ${rec.objectID}`));
          console.log(JSON.stringify(rec, null, 2));
        }

        usedIds[rec.objectID] = true;

        if (rec.level > 1 && rec.level < 6 && rec.tags.length == 0) {
          // The h2 -> h5 should have a set of tags that are the "path" within the file.
          console.log(chalk.yellow("Found h2 -> h5 with no tags."));
          console.log(JSON.stringify(rec, null, 2));
        }
      }

      if (index === null) {
        console.log(chalk.magenta("\nRecords for:"));
        console.log(chalk.cyan(file));
        console.log(JSON.stringify(algoliaRecords, null, 2));
      } else {
        for (const chnk of chunk(algoliaRecords, algoliaBatchSize)) {
          try {
            await index.saveObjects(chnk);
            console.log(chalk.cyan(file));
          } catch (err) {
            console.trace(err);
            process.exit(1);
          }
        }
      }
    }
  }

  console.log(
    chalk.green(`Success. Updated records for ${files.length} file(s).`)
  );
}

async function buildIndex() {
  dotEnv.config();

  const DEBUG = false;

  const algolia = algoliasearch(
    process.env.ALGOLIA_APP_ID || "",
    process.env.ALGOLIA_ADMIN_KEY
  );

  let algoliaIndex: SearchIndex | null = null;

  if (!DEBUG) {
    algoliaIndex = algolia.initIndex(algoliaIndexName);

    const exists = await algoliaIndex.exists();

    if (!exists) {
      console.error(`Index "${algoliaIndexName}" does not exist.`);
      process.exit(1);
    }

    await algoliaIndex.setSettings({
      ranking: [
        "typo",
        "geo",
        "words",
        "filters",
        "proximity",
        "attribute",
        "exact",
        "custom",
      ],
      customRanking: ["desc(level)", "desc(ranking)"],
      searchableAttributes: ["title", "content", "unordered(tags)"],
      attributesToSnippet: ["title:10", "content:10"],
      snippetEllipsisText: "…",
    });
  }

  let files = await glob(`${publicPath}/docs/about/**/**.html`);
  console.log(chalk.blue("Indexing docs/about..."));
  await indexHTMLFiles(algoliaIndex, "Docs", files, 50);
}

buildIndex().catch((err) => {
  console.trace(err);
});
