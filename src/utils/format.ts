export const formatNumberWithSetting = (num: number, setting: string): string => {
  if (setting === "us") {
    return num.toLocaleString("en-US");
  }
  if (setting === "eu") {
    return num.toLocaleString("de-DE");
  }
  if (setting === "fr") {
    return num.toLocaleString("fr-FR").replace(/\u202f/g, " ");
  }
  // "system" fallback
  return num.toLocaleString(undefined);
};

export const formatDateWithSetting = (dateObj: Date, setting: string): string => {
  if (setting === "iso") {
    const yyyy = dateObj.getFullYear();
    const mm = String(dateObj.getMonth() + 1).padStart(2, "0");
    const dd = String(dateObj.getDate()).padStart(2, "0");
    return `${yyyy}-${mm}-${dd}`;
  }
  if (setting === "us") {
    const yyyy = dateObj.getFullYear();
    const mm = String(dateObj.getMonth() + 1).padStart(2, "0");
    const dd = String(dateObj.getDate()).padStart(2, "0");
    return `${mm}/${dd}/${yyyy}`;
  }
  if (setting === "eu") {
    const yyyy = dateObj.getFullYear();
    const mm = String(dateObj.getMonth() + 1).padStart(2, "0");
    const dd = String(dateObj.getDate()).padStart(2, "0");
    return `${dd}/${mm}/${yyyy}`;
  }
  // "system" fallback
  return dateObj.toLocaleDateString(undefined);
};
