from PIL import Image, ImageDraw
import os

def crop_center_circle(input_dir, output_dir, final_size=48):
    """Crop to center circle and remove outer decorations"""
    os.makedirs(output_dir, exist_ok=True)
    
    icons = ["icon_idle.png", "icon_recording.png", "icon_processing.png"]
    
    for icon_name in icons:
        input_path = os.path.join(input_dir, icon_name)
        output_path = os.path.join(output_dir, icon_name)
        
        if os.path.exists(input_path):
            img = Image.open(input_path).convert("RGBA")
            w, h = img.size
            
            # Crop center 70% to remove outer decorations (black dots, rings)
            crop_ratio = 0.65
            margin = int(w * (1 - crop_ratio) / 2)
            img = img.crop((margin, margin, w - margin, h - margin))
            
            # Resize to final size
            img = img.resize((final_size, final_size), Image.Resampling.LANCZOS)
            
            # Create circular mask
            mask = Image.new("L", (final_size, final_size), 0)
            draw = ImageDraw.Draw(mask)
            draw.ellipse((0, 0, final_size, final_size), fill=255)
            
            # Apply mask
            output = Image.new("RGBA", (final_size, final_size), (0, 0, 0, 0))
            output.paste(img, (0, 0), mask)
            
            output.save(output_path, "PNG")
            print(f"Processed: {icon_name} -> cropped center, {final_size}x{final_size}")
        else:
            print(f"Not found: {input_path}")

if __name__ == "__main__":
    crop_center_circle("assets", "assets", 48)
    print("Done!")
